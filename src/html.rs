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
  
  // Detect number of CPU cores
  const numCores = navigator.hardwareConcurrency || 4;
  console.log(`Detected ${numCores} CPU cores`);

  function str2buf(str) {
    return new TextEncoder().encode(str);
  }

  async function sha256hex(input) {
    const hashBuffer = await crypto.subtle.digest("SHA-256", input);
    return Array.from(new Uint8Array(hashBuffer))
      .map(b => b.toString(16).padStart(2, '0'))
      .join('');
  }

  let globalNonce = 0;
  let solved = false;
  let totalHashes = 0;
  const startTime = Date.now();
  progressEl.hidden = false;

  function updateStatus(text) {
    statusEl.textContent = text;
  }

  // Worker code as a string to create inline workers
  const workerCode = `
    self.onmessage = async function(e) {
      const { challenge, difficultyPrefix, startNonce, chunkSize, workerId } = e.data;
      
      function str2buf(str) {
        return new TextEncoder().encode(str);
      }

      async function sha256hex(input) {
        const hashBuffer = await crypto.subtle.digest("SHA-256", input);
        return Array.from(new Uint8Array(hashBuffer))
          .map(b => b.toString(16).padStart(2, '0'))
          .join('');
      }

      let nonce = startNonce;
      let hashes = 0;
      
      for (let i = 0; i < chunkSize; i++) {
        const input = challenge + nonce.toString();
        const hash = await sha256hex(str2buf(input));
        hashes++;

        if (hash.startsWith(difficultyPrefix)) {
          self.postMessage({ 
            type: 'solution', 
            nonce: nonce, 
            hash: hash, 
            hashes: hashes,
            workerId: workerId 
          });
          return;
        }
        nonce++;
      }

      self.postMessage({ 
        type: 'progress', 
        lastNonce: nonce, 
        hashes: hashes,
        workerId: workerId 
      });
    };
  `;

  // Create workers
  const workers = [];
  const workerBlob = new Blob([workerCode], { type: 'application/javascript' });
  const workerUrl = URL.createObjectURL(workerBlob);

  for (let i = 0; i < numCores; i++) {
    const worker = new Worker(workerUrl);
    worker.workerId = i;
    workers.push(worker);
  }

  // Handle worker messages
  workers.forEach((worker, index) => {
    worker.onmessage = function(e) {
      const { type, nonce, hash, hashes, workerId } = e.data;
      
      totalHashes += hashes;
      
      if (type === 'solution' && !solved) {
        solved = true;
        const elapsed = (Date.now() - startTime) / 1000;
        const hashRate = Math.round(totalHashes / elapsed);
        
        updateStatus(`Solved by core ${workerId}! Nonce: ${nonce}, Hash: ${hash} (${hashRate} H/s)`);
        progressEl.value = 100;
        
        // Terminate all workers
        workers.forEach(w => w.terminate());
        URL.revokeObjectURL(workerUrl);
        
        submitSolution(nonce);
        return;
      }
      
      if (type === 'progress' && !solved) {
        // Update global nonce to the highest processed
        globalNonce = Math.max(globalNonce, e.data.lastNonce);
        
        const elapsed = (Date.now() - startTime) / 1000;
        const hashRate = elapsed > 0 ? Math.round(totalHashes / elapsed) : 0;
        
        progressEl.value = (globalNonce % 100000) / 1000 % 100;
        updateStatus(`Mining with ${numCores} cores... ${hashRate} H/s (nonce: ${globalNonce})`);
        
        // Assign next chunk to this worker
        const chunkSize = 1000;
        const startNonce = globalNonce + (workerId * chunkSize);
        globalNonce += numCores * chunkSize;
        
        worker.postMessage({
          challenge: challenge,
          difficultyPrefix: difficultyPrefix,
          startNonce: startNonce,
          chunkSize: chunkSize,
          workerId: workerId
        });
      }
    };
  });

  function submitSolution(nonce) {
    const params = new URLSearchParams();
    params.append('nonce', nonce.toString());
    params.append('token', token);
    fetch("/post_nonce", {
      method: "POST",
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded'
      },
      body: params.toString()
    })
    .then(res => {
      if (res.ok) {
        updateStatus(statusEl.textContent + " – Server accepted solution!");
        setTimeout(() => {
          window.location.href = "/validate";
        }, 1000);
      } else {
        updateStatus(statusEl.textContent + " – Server rejected solution.");
      }
    }).catch(() => {
      updateStatus(statusEl.textContent + " – Failed to submit solution.");
    });
  }
  updateStatus(`Starting mining with ${numCores} CPU cores...`);
  const chunkSize = 1000;
  workers.forEach((worker, index) => {
    const startNonce = globalNonce + (index * chunkSize);
    worker.postMessage({
      challenge: challenge,
      difficultyPrefix: difficultyPrefix,
      startNonce: startNonce,
      chunkSize: chunkSize,
      workerId: index
    });
  });
  globalNonce += numCores * chunkSize;
})();
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
