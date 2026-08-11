/**
 * Interfold Studio Client Logic
 */

let activeE3Id = null;

document.addEventListener('DOMContentLoaded', () => {
  initTabs();
  loadConfig();
  initListeners();
});

function initTabs() {
  const tabs = document.querySelectorAll('.nav-tab');
  tabs.forEach(tab => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.nav-tab').forEach(t => t.classList.toggle('active', t === tab));
      document.querySelectorAll('.tab-pane').forEach(p => p.classList.toggle('active', p.id === `tab-${tab.dataset.tab}`));
    });
  });
}

async function loadConfig() {
  try {
    const res = await fetch('/api/config');
    const data = await res.json();

    const select = document.getElementById('select-program');
    select.innerHTML = '';

    data.programs.forEach(p => {
      const opt = document.createElement('option');
      opt.value = p.id;
      opt.textContent = `${p.title} (${p.inputWindowDuration / 60}m window)`;
      select.appendChild(opt);
    });
  } catch (e) {
    console.error(e);
  }
}

function initListeners() {
  // Request E3
  document.getElementById('e3-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const programId = document.getElementById('select-program').value;
    const presetName = document.getElementById('select-preset').value;
    const resultBox = document.getElementById('e3-result-box');

    try {
      const res = await fetch('/api/e3/request', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ programId, presetName }),
      });
      const data = await res.json();
      activeE3Id = data.e3.e3Id;

      resultBox.innerHTML = `
        <div class="card" style="border-color: #3b82f6; background: rgba(59, 130, 246, 0.08);">
          <strong style="color: #93c5fd;">🔒 E3 Instance #${data.e3.e3Id} Activated!</strong>
          <div class="mono text-muted mt-1" style="font-size: 0.75rem;">Stage: ${data.e3.stage} • PK Commitment: ${data.e3.pkCommitment}</div>
        </div>
      `;
    } catch (err) {
      resultBox.innerHTML = `<div class="badge red">Request error: ${err.message}</div>`;
    }
  });

  // Encrypt Input
  document.getElementById('encrypt-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const valueNumber = parseInt(document.getElementById('input-number').value);
    const box = document.getElementById('encrypt-json-box');

    try {
      const res = await fetch('/api/e3/encrypt', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ e3Id: activeE3Id, valueNumber }),
      });
      const data = await res.json();
      box.textContent = JSON.stringify(data.result, null, 2);
    } catch (err) {
      box.textContent = `Error: ${err.message}`;
    }
  });

  // Decode Plaintext Hex Output
  document.getElementById('decode-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const hex = document.getElementById('decode-hex').value;
    const box = document.getElementById('decode-json-box');

    try {
      const res = await fetch('/api/e3/decode', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ hex }),
      });
      const data = await res.json();
      box.textContent = JSON.stringify(data.decoded, null, 2);
    } catch (err) {
      box.textContent = `Error: ${err.message}`;
    }
  });
}
