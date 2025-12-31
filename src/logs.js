import {
  invoke,
  logsTimeRange,
  setLogsTimeRange,
  logsBackend,
  setLogsBackend,
  currentLogs,
  setCurrentLogs,
  formatNumber,
  formatLatency,
  formatTimestamp,
  shortenModel,
  escapeHtml
} from './utils.js';

// Render message logs table
function renderLogsTable(logs) {
  setCurrentLogs(logs);

  if (logs.length === 0) {
    return `
      <div class="empty-state">
        <h3>No logs yet</h3>
        <p>Make some API requests through the proxy to see logs here.</p>
      </div>
    `;
  }

  return `
    <div class="logs-table-container">
      <table class="logs-table">
        <thead>
          <tr>
            <th>Time</th>
            <th>Backend</th>
            <th>Model</th>
            <th>Tokens</th>
            <th>Latency</th>
            <th>Request</th>
            <th>Response</th>
          </tr>
        </thead>
        <tbody>
          ${logs.map((log, index) => `
            <tr>
              <td class="col-time">${formatTimestamp(log.timestamp)}</td>
              <td class="col-backend">${log.backend}</td>
              <td class="col-model">${shortenModel(log.model)}</td>
              <td class="col-tokens">${formatNumber(log.input_tokens)} / ${formatNumber(log.output_tokens)}</td>
              <td class="col-latency">${formatLatency(log.latency_ms)}</td>
              <td class="col-json">
                <button class="json-btn" data-index="${index}" data-type="request">View</button>
              </td>
              <td class="col-json">
                <button class="json-btn" data-index="${index}" data-type="response">View</button>
              </td>
            </tr>
          `).join('')}
        </tbody>
      </table>
    </div>
  `;
}

// Format JSON string for display
function formatJson(jsonStr) {
  try {
    const parsed = JSON.parse(jsonStr);
    return JSON.stringify(parsed, null, 2);
  } catch {
    return jsonStr || 'null';
  }
}

// Show JSON modal with tabbed view (Headers/Body)
function showJsonModal(title, headersStr, bodyStr) {
  const existing = document.getElementById('json-modal');
  if (existing) existing.remove();

  const formattedHeaders = formatJson(headersStr);
  const formattedBody = formatJson(bodyStr);

  const modal = document.createElement('div');
  modal.id = 'json-modal';
  modal.className = 'json-modal';
  modal.innerHTML = `
    <div class="json-modal-content">
      <div class="json-modal-header">
        <h3>${title}</h3>
        <button class="json-modal-close">&times;</button>
      </div>
      <div class="json-modal-tabs">
        <button class="json-modal-tab active" data-tab="headers">Headers</button>
        <button class="json-modal-tab" data-tab="body">Body</button>
      </div>
      <div class="json-modal-tab-content">
        <pre class="json-modal-body tab-panel active" data-panel="headers">${escapeHtml(formattedHeaders)}</pre>
        <pre class="json-modal-body tab-panel" data-panel="body">${escapeHtml(formattedBody)}</pre>
      </div>
    </div>
  `;
  document.body.appendChild(modal);

  // Tab switching logic
  modal.querySelectorAll('.json-modal-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      const tabName = tab.dataset.tab;
      // Update active tab button
      modal.querySelectorAll('.json-modal-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      // Update active panel
      modal.querySelectorAll('.tab-panel').forEach(p => p.classList.remove('active'));
      modal.querySelector(`.tab-panel[data-panel="${tabName}"]`).classList.add('active');
    });
  });

  modal.querySelector('.json-modal-close').addEventListener('click', () => modal.remove());
  modal.addEventListener('click', (e) => {
    if (e.target === modal) modal.remove();
  });
}

// Load message logs
export async function loadMessageLogs() {
  const content = document.getElementById('logs-content');
  content.innerHTML = '<p class="loading">Loading...</p>';

  try {
    const logs = await invoke('get_message_logs', { timeRange: logsTimeRange, backend: logsBackend });
    content.innerHTML = renderLogsTable(logs);

    // Add click handlers for JSON buttons
    content.querySelectorAll('.json-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const index = parseInt(btn.dataset.index);
        const type = btn.dataset.type;
        const log = currentLogs[index];
        const title = type === 'request' ? `Request #${log.id}` : `Response #${log.id}`;
        const headersStr = type === 'request' ? log.request_headers : log.response_headers;
        const bodyStr = type === 'request' ? log.request_body : log.response_body;
        showJsonModal(title, headersStr, bodyStr);
      });
    });
  } catch (error) {
    content.innerHTML = `
      <div class="empty-state">
        <h3>Error loading logs</h3>
        <p>${error}</p>
      </div>
    `;
  }
}

// Load backends for logs tab
export async function loadLogsBackends() {
  try {
    const backends = await invoke('get_backends');
    const select = document.getElementById('logs-backend-select');
    select.innerHTML = '<option value="all">All Backends</option>';
    backends.forEach(backend => {
      const option = document.createElement('option');
      option.value = backend;
      option.textContent = backend.charAt(0).toUpperCase() + backend.slice(1);
      select.appendChild(option);
    });
  } catch (error) {
    console.error('Failed to load backends:', error);
  }
}

// Initialize logs backend filter
export function initLogsBackendFilter() {
  const select = document.getElementById('logs-backend-select');
  select.addEventListener('change', () => {
    setLogsBackend(select.value);
    loadMessageLogs();
  });
}

// Initialize logs time filter
export function initLogsTimeFilter() {
  const select = document.getElementById('logs-time-select');
  select.addEventListener('change', () => {
    setLogsTimeRange(select.value);
    loadMessageLogs();
  });
}
