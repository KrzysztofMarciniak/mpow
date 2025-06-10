use html_escape::encode_text;

pub const STYLE_CSS: &str = r#"
:root {
  --color-primary: #1e90ff;
  --color-primary-glow: #00c0ff;
  --bg-color: #000;
  --text-color: #fff;
  --code-bg: #222;
  --atom-size: 120px;
}
body {
  background-color: var(--bg-color);
  color: var(--text-color);
  font-family: Arial, sans-serif;
  text-align: center;
  margin: 0;
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
}
h1 a {
  color: var(--color-primary);
  text-decoration: none;
}
h1 a:hover { text-decoration: underline; }
code {
  background-color: var(--code-bg);
  padding: 0.2em 0.4em;
  border-radius: 4px;
  font-family: monospace, monospace;
}
.atom {
  position: relative;
  width: var(--atom-size);
  height: var(--atom-size);
  margin: 1em auto;
}
.nucleus {
  width: calc(var(--atom-size) * 0.1667);
  height: calc(var(--atom-size) * 0.1667);
  background: radial-gradient(circle at center, var(--color-primary), #004080);
  border-radius: 50%;
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  box-shadow: 0 0 8px 2px var(--color-primary-glow)88, 0 0 15px 5px var(--color-primary-glow)44;
}
.orbit {
  width: 100%;
  height: 100%;
  border: 1px dashed var(--color-primary-glow);
  border-radius: 50%;
  position: absolute;
  top: 0;
  left: 0;
  animation: rotateOrbit 4s linear infinite;
}
.electron {
  width: calc(var(--atom-size) * 0.0833);
  height: calc(var(--atom-size) * 0.0833);
  background: var(--color-primary);
  border-radius: 50%;
  position: absolute;
  top: 50%;
  left: 0;
  transform: translate(-50%, -50%);
  box-shadow: 0 0 6px 2px var(--color-primary-glow)88, 0 0 10px 3px var(--color-primary-glow)44;
}
@keyframes rotateOrbit {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
#status {
  margin-top: 1em;
  font-size: 1.1em;
  min-height: 1.5em;
}
progress {
  width: 200px;
  height: 1em;
  margin-top: 0.5em;
  accent-color: var(--color-primary);
}
"#;

pub const JS_SCRIPT: &str = r#"
(async () => {
  const statusEl = document.getElementById("status");
  const progressEl = document.getElementById("progress");
  
  const numCores = navigator.hardwareConcurrency || 4;
  console.log(`Detected ${numCores} CPU cores`);

  let globalNonce = 0;
  let solved = false;
  let totalHashes = 0;
  const startTime = Date.now();
  let lastUpdateTime = startTime;
  progressEl.hidden = false;

  const updateStatus = text => statusEl.textContent = text;

  // Improved worker code with better buffer management
  const workerCode = `
    self.onmessage = async function(e) {
      const { challenge, difficultyPrefix, startNonce, chunkSize, workerId } = e.data;
      
      const encoder = new TextEncoder();
      const challengeBuf = encoder.encode(challenge);
      const difficultyLen = difficultyPrefix.length;

      // Pre-allocate buffer to avoid repeated allocations
      const maxNonceLen = 20; // Reasonable max for nonce string length
      const workBuffer = new Uint8Array(challengeBuf.length + maxNonceLen);
      workBuffer.set(challengeBuf);

      let nonce = startNonce;
      let hashes = 0;
      const batchSize = 100; // Process in smaller batches for better responsiveness
      
      while (hashes < chunkSize) {
        const batchEnd = Math.min(hashes + batchSize, chunkSize);
        
        for (let i = hashes; i < batchEnd; i++) {
          const nonceStr = nonce.toString();
          const nonceBytes = encoder.encode(nonceStr);
          workBuffer.set(nonceBytes, challengeBuf.length);
          
          const hashBuffer = await crypto.subtle.digest("SHA-256", 
            workBuffer.subarray(0, challengeBuf.length + nonceBytes.length));
          
          // Fast prefix check without string conversion
          const hashArray = new Uint8Array(hashBuffer);
          let matches = true;
          for (let j = 0; j < Math.ceil(difficultyLen / 2); j++) {
            const byte = hashArray[j];
            const hex1 = (byte >> 4).toString(16);
            const hex2 = (byte & 0xf).toString(16);
            
            if (j * 2 < difficultyLen && hex1 !== difficultyPrefix[j * 2]) {
              matches = false;
              break;
            }
            if (j * 2 + 1 < difficultyLen && hex2 !== difficultyPrefix[j * 2 + 1]) {
              matches = false;
              break;
            }
          }

          if (matches) {
            // Only convert to hex string when we have a match
            const hash = Array.from(hashArray)
              .map(b => b.toString(16).padStart(2, '0'))
              .join('');
            
            self.postMessage({ type: 'solution', nonce, hash, hashes: hashes + 1, workerId });
            return;
          }
          nonce++;
        }
        
        hashes = batchEnd;
        
        // Yield control periodically
        if (hashes % (batchSize * 10) === 0) {
          await new Promise(resolve => setTimeout(resolve, 0));
        }
      }

      self.postMessage({ type: 'progress', lastNonce: nonce, hashes, workerId });
    };
  `;

  const workerBlob = new Blob([workerCode], { type: 'application/javascript' });
  const workerUrl = URL.createObjectURL(workerBlob);
  const workers = Array.from({ length: numCores }, (_, i) => {
    const worker = new Worker(workerUrl);
    worker.workerId = i;
    return worker;
  });

  // Adaptive chunk size based on performance
  let chunkSize = 5000;
  const minChunkSize = 1000;
  const maxChunkSize = 50000;

  workers.forEach((worker, index) => {
    worker.onmessage = function(e) {
      const { type, nonce, hash, hashes, workerId } = e.data;
      
      totalHashes += hashes;
      
      if (type === 'solution' && !solved) {
        solved = true;
        const elapsed = (Date.now() - startTime) / 1000;
        const hashRate = Math.round(totalHashes / elapsed);
        
        updateStatus(`✅ Solved by core ${workerId}! Nonce: ${nonce} (${hashRate.toLocaleString()} H/s)`);
        progressEl.value = 100;
        
        workers.forEach(w => w.terminate());
        URL.revokeObjectURL(workerUrl);
        
        submitSolution(nonce);
        return;
      }
      
      if (type === 'progress' && !solved) {
        globalNonce = Math.max(globalNonce, e.data.lastNonce);
        
        // Throttle UI updates to improve performance
        const now = Date.now();
          const elapsed = (now - startTime) / 1000;
          const hashRate = elapsed > 0 ? Math.round(totalHashes / elapsed) : 0;
          
          // Adaptive chunk size based on hash rate
          if (hashRate > 0) {
            if (hashRate < 1000 && chunkSize > minChunkSize) {
              chunkSize = Math.max(minChunkSize, chunkSize * 0.8);
            } else if (hashRate > 5000 && chunkSize < maxChunkSize) {
              chunkSize = Math.min(maxChunkSize, chunkSize * 1.2);
            }
          }
          
          progressEl.value = (globalNonce % 100000) / 1000 % 100;
          updateStatus(`⚡ Mining with ${numCores} cores... ${hashRate.toLocaleString()} H/s (nonce: ${globalNonce.toLocaleString()})`);
          lastUpdateTime = now;
        
        
        const startNonce = globalNonce + (workerId * chunkSize);
        globalNonce += numCores * chunkSize;
        
        worker.postMessage({
          challenge,
          difficultyPrefix,
          startNonce,
          chunkSize: Math.floor(chunkSize),
          workerId
        });
      }
    };
  });

  async function submitSolution(nonce) {
    try {
      updateStatus(statusEl.textContent + " 📤 Submitting...");
      
      const controller = new AbortController();
      const timeoutId = setTimeout(() => controller.abort(), 10000); // 10s timeout
      
      const params = new URLSearchParams();
      params.append('nonce', nonce.toString());
      params.append('token', token);
      
      const res = await fetch("/post_nonce", {
        method: "POST",
        headers: {'Content-Type': 'application/x-www-form-urlencoded'},
        body: params.toString(),
        signal: controller.signal
      });
      
      clearTimeout(timeoutId);
      
      if (res.ok) {
        updateStatus(statusEl.textContent.replace("📤 Submitting...", "") + " ✅ Server accepted!");
        setTimeout(() => window.location.href = "/validate", 1500);
      } else {
        const errorText = await res.text().catch(() => 'Unknown error');
        updateStatus(statusEl.textContent.replace("📤 Submitting...", "") + ` ❌ Server rejected: ${errorText}`);
      }
    } catch (error) {
      const message = error.name === 'AbortError' ? 'Request timeout' : 'Network error';
      updateStatus(statusEl.textContent.replace("📤 Submitting...", "") + ` ❌ ${message}`);
    }
  }

  // Graceful shutdown on page unload
  window.addEventListener('beforeunload', () => {
    workers.forEach(w => w.terminate());
    URL.revokeObjectURL(workerUrl);
  });

  updateStatus(`🚀 Starting mining with ${numCores} CPU cores...`);
  workers.forEach((worker, index) => {
    worker.postMessage({
      challenge,
      difficultyPrefix,
      startNonce: globalNonce + (index * chunkSize),
      chunkSize: Math.floor(chunkSize),
      workerId: index
    });
  });
  globalNonce += numCores * chunkSize;
})().catch(error => {
  console.error('Mining error:', error);
  document.getElementById("status").textContent = `❌ Error: ${error.message}`;
});
"#;

pub fn generate_challenge_html(token: &str, challenge: &str, difficulty: usize) -> String {
	let sanitized_challenge = encode_text(challenge);
	let sanitized_token = encode_text(token);
	let difficulty_prefix = "0".repeat(difficulty);

	format!(
		r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Minimal Proof of Work</title>
<style>{style}</style>
</head>
<body>
<h1>
  <a href="https://github.com/krzysztofmarciniak/mpow" target="_blank" rel="noopener noreferrer">
    Minimal Proof of Work Challenge
  </a>
</h1>
<p>Challenge string: <code>{challenge}</code></p>

<div class="atom">
  <div class="nucleus"></div>
  <div class="orbit">
    <div class="electron"></div>
  </div>
</div>

<noscript>You have to have Javascript enabled to complete verification.</noscript>
<p id="status" aria-live="polite">Solving challenge...</p>
<progress id="progress" max="100" value="0" hidden></progress>

<script>
  const challenge = "{challenge}";
  const token = "{token}";
  const difficultyPrefix = "{difficulty_prefix}";
</script>
<script>{js}</script>
</body>
</html>"#,
		challenge = sanitized_challenge,
		token = sanitized_token,
		difficulty_prefix = difficulty_prefix,
		style = STYLE_CSS,
		js = JS_SCRIPT,
	)
}

pub fn render_challenge_page(challenge: &str, difficulty: &str) -> String {
	let sanitized_challenge = encode_text(challenge);
	let sanitized_difficulty = encode_text(difficulty);
	format!(
		r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Minimal Proof of Work</title>
<style>{style}</style>
</head>
<body>
<h1>
  <a href="https://github.com/krzysztofmarciniak/mpow" target="_blank" rel="noopener noreferrer">
    Minimal Proof of Work Challenge
  </a>
</h1>
<p>Challenge string: <code>{challenge}</code></p>

<div class="atom">
  <div class="nucleus"></div>
  <div class="orbit">
    <div class="electron"></div>
  </div>
</div>

<noscript>You have to have Javascript enabled to complete verification.</noscript>
<p id="status" aria-live="polite">Solving challenge...</p>
<progress id="progress" max="100" value="0" hidden></progress>

<script>
  const challenge = "{challenge}";
  const difficultyPrefix = "{difficulty}";
</script>
<script>{js}</script>
</body>
</html>"#,
		challenge = sanitized_challenge,
		difficulty = sanitized_difficulty,
		style = STYLE_CSS,
		js = JS_SCRIPT,
	)
}

/// Debug helper
pub fn demo_html() {
	println!("html module demo called");
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn demo_function_exists_html() {
		demo_html();
	}

	#[test]
	fn test_render_challenge_page() {
		let challenge = "test_challenge";
		let difficulty = "00";
		let rendered = render_challenge_page(challenge, difficulty);

		assert!(rendered.contains("test_challenge"));
		assert!(rendered.contains("00"));
		assert!(rendered.contains("<!DOCTYPE html>"));
		assert!(rendered.contains("<html lang=\"en\">"));
		assert!(rendered.contains(STYLE_CSS));
		assert!(rendered.contains(JS_SCRIPT));
	}

	#[test]
	fn test_generate_challenge_html() {
		let token = "test_token";
		let challenge = "test_challenge";
		let difficulty = 4;
		let rendered = generate_challenge_html(token, challenge, difficulty);

		assert!(rendered.contains("test_token"));
		assert!(rendered.contains("test_challenge"));
		assert!(rendered.contains("0000"));
		assert!(rendered.contains("<!DOCTYPE html>"));
	}
}
