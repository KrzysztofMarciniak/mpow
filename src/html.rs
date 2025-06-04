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
  const difficultyPrefix = "0000";

  function str2buf(str) {
    return new TextEncoder().encode(str);
  }

  async function sha256hex(input) {
    const hashBuffer = await crypto.subtle.digest("SHA-256", input);
    return Array.from(new Uint8Array(hashBuffer))
      .map(b => b.toString(16).padStart(2, '0'))
      .join('');
  }

  let nonce = 0;
  const chunkSize = 1000;
  let solved = false;
  progressEl.hidden = false;

  function updateStatus(text) {
    statusEl.textContent = text;
  }

  async function solveChunk() {
    for (let i = 0; i < chunkSize; i++) {
      const testStr = challenge + nonce.toString();
      const hash = await sha256hex(str2buf(testStr));
      if (hash.startsWith(difficultyPrefix)) {
        solved = true;
        updateStatus(`Solved! Nonce: ${nonce}, Hash: ${hash}`);
        progressEl.value = 100;
        submitSolution(nonce);
        return;
      }
      nonce++;
    }
    progressEl.value = (nonce % 100000) / 1000 % 100;
    updateStatus(`Trying nonce ${nonce}...`);
    if (!solved) requestAnimationFrame(solveChunk);
  }

  function submitSolution(nonce) {
    fetch("/", {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded"
      },
      body: "nonce=" + nonce
    })
    .then(res => {
      if (res.ok) {
        updateStatus(statusEl.textContent + " – Server accepted solution!");
        window.location.reload();
      } else {
        updateStatus(statusEl.textContent + " – Server rejected solution.");
      }
    }).catch(() => {
      updateStatus(statusEl.textContent + " – Failed to submit solution.");
    });
  }

  solveChunk();
})();
"#;
pub fn get_html_template(challenge: &str) -> String {
    let sanitized_challenge = encode_text(challenge);
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

  <p id="status" aria-live="polite">Solving challenge...</p>
  <progress id="progress" max="100" value="0" hidden></progress>

  <script>
    const challenge = "{challenge}";
  </script>
  <script>{js}</script>
</body>
</html>"#,
        challenge = sanitized_challenge,
        style = STYLE_CSS,
        js = JS_SCRIPT,
    )
}
