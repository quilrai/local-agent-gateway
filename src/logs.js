import {
  invoke,
  logsTimeRange,
  setLogsTimeRange,
  logsBackend,
  setLogsBackend,
  logsModel,
  setLogsModel,
  logsDlpAction,
  setLogsDlpAction,
  logsPage,
  setLogsPage,
  currentLogs,
  setCurrentLogs,
  formatNumber,
  formatLatency,
  formatRelativeTime,
  shortenModel,
  escapeHtml
} from './utils.js';

// Get DLP status info
function getDlpStatus(dlpAction) {
  switch (dlpAction) {
    case 2: return { label: 'Blocked', class: 'blocked' };
    case 1: return { label: 'Redacted', class: 'redacted' };
    default: return { label: 'Passed', class: 'passed' };
  }
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

// Render a single log card
function renderLogCard(log, index, cardNum, total) {
  const status = getDlpStatus(log.dlp_action);

  return `
    <div class="log-card" data-index="${index}">
      <div class="log-card-header">
        <span class="log-number">${cardNum}/${total}</span>
        <span class="log-time">${formatRelativeTime(log.timestamp)}</span>
        <span class="log-pill backend">${log.backend}</span>
        <span class="log-pill model">${shortenModel(log.model)}</span>
        <span class="log-pill status ${status.class}">${status.label}</span>
      </div>
      <div class="log-card-stats">
        <span class="stat"><strong>Latency:</strong> ${formatLatency(log.latency_ms)}</span>
        <span class="stat"><strong>In:</strong> ${formatNumber(log.input_tokens)}</span>
        <span class="stat"><strong>Out:</strong> ${formatNumber(log.output_tokens)}</span>
      </div>
      <div class="log-card-tabs">
        <button class="log-tab active" data-tab="data" data-index="${index}">Data</button>
        <button class="log-tab" data-tab="headers" data-index="${index}">Headers</button>
      </div>
      <div class="log-card-subtabs">
        <button class="log-subtab active" data-subtab="request" data-index="${index}">Request</button>
        <button class="log-subtab" data-subtab="response" data-index="${index}">Response</button>
        <button class="log-copy-btn" data-index="${index}" title="Copy request & response">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
          </svg>
        </button>
      </div>
      <div class="log-card-content">
        <pre class="log-json" data-index="${index}">${escapeHtml(formatJson(log.request_body))}</pre>
      </div>
    </div>
  `;
}

// Render message logs as cards
function renderLogsCards(logs, total) {
  setCurrentLogs(logs);

  if (logs.length === 0 && logsPage === 0) {
    return `
      <div class="empty-state">
        <h3>No logs yet</h3>
        <p>Make some API requests through the proxy to see logs here.</p>
      </div>
    `;
  }

  const totalPages = Math.ceil(total / 50);
  const currentPage = logsPage + 1;
  const startNum = logsPage * 50 + 1;

  return `
    <div class="pagination">
      <button class="pagination-btn" id="logs-prev" ${logsPage === 0 ? 'disabled' : ''}>Previous</button>
      <span class="pagination-info">Page ${currentPage} of ${totalPages}</span>
      <button class="pagination-btn" id="logs-next" ${currentPage >= totalPages ? 'disabled' : ''}>Next</button>
    </div>
    <div class="logs-grid">
      ${logs.map((log, index) => renderLogCard(log, index, startNum + index, total)).join('')}
    </div>
  `;
}

// Update card content based on current tab/subtab state
function updateCardContent(card, index) {
  const log = currentLogs[index];
  const activeTab = card.querySelector('.log-tab.active').dataset.tab;
  const activeSubtab = card.querySelector('.log-subtab.active').dataset.subtab;
  const jsonPre = card.querySelector('.log-json');

  let content;
  if (activeTab === 'data') {
    content = activeSubtab === 'request' ? log.request_body : log.response_body;
  } else {
    content = activeSubtab === 'request' ? log.request_headers : log.response_headers;
  }

  jsonPre.textContent = formatJson(content);
}

// Copy both request and response as tuple
function copyLogData(index, tab) {
  const log = currentLogs[index];
  let data;

  if (tab === 'data') {
    data = {
      request: JSON.parse(log.request_body || '{}'),
      response: JSON.parse(log.response_body || '{}')
    };
  } else {
    data = {
      request: JSON.parse(log.request_headers || '{}'),
      response: JSON.parse(log.response_headers || '{}')
    };
  }

  navigator.clipboard.writeText(JSON.stringify(data, null, 2)).then(() => {
    // Brief visual feedback could be added here
  }).catch(err => {
    console.error('Failed to copy:', err);
  });
}

// Attach event handlers to log cards
function attachCardHandlers(container) {
  // Tab switching (Data/Headers)
  container.querySelectorAll('.log-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      const card = tab.closest('.log-card');
      const index = parseInt(tab.dataset.index);

      card.querySelectorAll('.log-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      updateCardContent(card, index);
    });
  });

  // Subtab switching (Request/Response)
  container.querySelectorAll('.log-subtab').forEach(subtab => {
    subtab.addEventListener('click', () => {
      const card = subtab.closest('.log-card');
      const index = parseInt(subtab.dataset.index);

      card.querySelectorAll('.log-subtab').forEach(t => t.classList.remove('active'));
      subtab.classList.add('active');
      updateCardContent(card, index);
    });
  });

  // Copy button
  container.querySelectorAll('.log-copy-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const card = btn.closest('.log-card');
      const index = parseInt(btn.dataset.index);
      const activeTab = card.querySelector('.log-tab.active').dataset.tab;
      copyLogData(index, activeTab);

      // Visual feedback
      btn.classList.add('copied');
      setTimeout(() => btn.classList.remove('copied'), 1000);
    });
  });
}

// Load message logs
export async function loadMessageLogs() {
  const content = document.getElementById('logs-content');
  content.innerHTML = '<p class="loading">Loading...</p>';

  try {
    const result = await invoke('get_message_logs', {
      timeRange: logsTimeRange,
      backend: logsBackend,
      model: logsModel,
      dlpAction: logsDlpAction,
      page: logsPage
    });
    content.innerHTML = renderLogsCards(result.logs, result.total);
    attachCardHandlers(content);
    attachPaginationHandlers(content);
  } catch (error) {
    content.innerHTML = `
      <div class="empty-state">
        <h3>Error loading logs</h3>
        <p>${error}</p>
      </div>
    `;
  }
}

// Attach pagination handlers
function attachPaginationHandlers(container) {
  const prevBtn = container.querySelector('#logs-prev');
  const nextBtn = container.querySelector('#logs-next');

  if (prevBtn) {
    prevBtn.addEventListener('click', () => {
      if (logsPage > 0) {
        setLogsPage(logsPage - 1);
        loadMessageLogs();
      }
    });
  }

  if (nextBtn) {
    nextBtn.addEventListener('click', () => {
      setLogsPage(logsPage + 1);
      loadMessageLogs();
    });
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
    setLogsPage(0);
    loadMessageLogs();
  });
}

// Initialize logs time filter
export function initLogsTimeFilter() {
  const select = document.getElementById('logs-time-select');
  select.addEventListener('change', () => {
    setLogsTimeRange(select.value);
    setLogsPage(0);
    loadMessageLogs();
  });
}

// Load models for logs tab
export async function loadLogsModels() {
  try {
    const models = await invoke('get_models');
    const select = document.getElementById('logs-model-select');
    select.innerHTML = '<option value="all">All Models</option>';
    models.forEach(model => {
      const option = document.createElement('option');
      option.value = model;
      option.textContent = shortenModel(model);
      select.appendChild(option);
    });
  } catch (error) {
    console.error('Failed to load models:', error);
  }
}

// Initialize logs model filter
export function initLogsModelFilter() {
  const select = document.getElementById('logs-model-select');
  select.addEventListener('change', () => {
    setLogsModel(select.value);
    setLogsPage(0);
    loadMessageLogs();
  });
}

// Initialize logs DLP filter
export function initLogsDlpFilter() {
  const select = document.getElementById('logs-dlp-select');
  select.addEventListener('change', () => {
    setLogsDlpAction(select.value);
    setLogsPage(0);
    loadMessageLogs();
  });
}
