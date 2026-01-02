import {
  invoke,
  currentTimeRange,
  setCurrentTimeRange,
  currentBackend,
  setCurrentBackend,
  formatNumber
} from './utils.js';
import { destroyCharts, createModelsChart, createTokenChart, createLatencyChart, createDlpChart } from './charts.js';

// Render dashboard HTML
function renderDashboard(data, dlpStats) {
  const { models, features, token_totals, recent_requests, latency_points } = data;

  const pct = (val) => features.total_requests > 0 ? Math.round((val / features.total_requests) * 100) : 0;

  return `
    <div class="charts-grid">
      <!-- Models Chart -->
      <div class="card">
        <div class="card-header">
          Models Used
          <span class="badge">${models.length} models</span>
        </div>
        <div class="card-body chart-container-sm" id="models-chart"></div>
      </div>

      <!-- Token Totals -->
      <div class="card">
        <div class="card-header">Token Totals</div>
        <div class="card-body">
          <div class="token-stats">
            <div class="token-stat input">
              <div class="value">${formatNumber(token_totals.input)}</div>
              <div class="label">Input</div>
            </div>
            <div class="token-stat output">
              <div class="value">${formatNumber(token_totals.output)}</div>
              <div class="label">Output</div>
            </div>
            <div class="token-stat cache-read">
              <div class="value">${formatNumber(token_totals.cache_read)}</div>
              <div class="label">Cache Read</div>
            </div>
            <div class="token-stat cache-create">
              <div class="value">${formatNumber(token_totals.cache_creation)}</div>
              <div class="label">Cache Create</div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="charts-grid">
      <!-- Request Features -->
      <div class="card">
        <div class="card-header">Request Features</div>
        <div class="card-body">
          <div class="feature-bars">
            <div class="feature-bar">
              <div class="feature-label">
                <span class="feature-name">System Prompt</span>
                <span class="feature-value">${features.with_system_prompt} (${pct(features.with_system_prompt)}%)</span>
              </div>
              <div class="bar-track">
                <div class="bar-fill system" style="width: ${pct(features.with_system_prompt)}%"></div>
              </div>
            </div>
            <div class="feature-bar">
              <div class="feature-label">
                <span class="feature-name">Tools</span>
                <span class="feature-value">${features.with_tools} (${pct(features.with_tools)}%)</span>
              </div>
              <div class="bar-track">
                <div class="bar-fill tools" style="width: ${pct(features.with_tools)}%"></div>
              </div>
            </div>
            <div class="feature-bar">
              <div class="feature-label">
                <span class="feature-name">Thinking</span>
                <span class="feature-value">${features.with_thinking} (${pct(features.with_thinking)}%)</span>
              </div>
              <div class="bar-track">
                <div class="bar-fill thinking" style="width: ${pct(features.with_thinking)}%"></div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Latency Chart -->
      <div class="card">
        <div class="card-header">
          Latency Trend
          <span class="badge">last ${latency_points.length}</span>
        </div>
        <div class="card-body chart-container-sm" id="latency-chart"></div>
      </div>
    </div>

    <!-- Token Usage Per Request -->
    <div class="charts-grid">
      <div class="card full-width">
        <div class="card-header">
          Token Usage Per Request
          <span class="badge">last ${Math.min(recent_requests.length, 15)} requests</span>
        </div>
        <div class="card-body chart-container" id="token-chart"></div>
      </div>
    </div>

    <!-- DLP Detections -->
    <div class="charts-grid">
      <div class="card full-width">
        <div class="card-header">
          DLP Detections
        </div>
        <div class="card-body">
          <div class="dlp-chart-container" id="dlp-chart">
            ${(!dlpStats || dlpStats.detections_by_pattern.length === 0) ? '<p class="empty-text">No detections</p>' : ''}
          </div>
        </div>
      </div>
    </div>
  `;
}

// Load dashboard
export async function loadDashboard() {
  const content = document.getElementById('dashboard-content');
  content.innerHTML = '<p class="loading">Loading...</p>';

  destroyCharts();

  try {
    // Load dashboard stats and DLP stats in parallel
    const [data, dlpStats] = await Promise.all([
      invoke('get_dashboard_stats', { timeRange: currentTimeRange, backend: currentBackend }),
      invoke('get_dlp_detection_stats', { timeRange: currentTimeRange })
    ]);

    if (data.total_requests === 0 && dlpStats.total_detections === 0) {
      content.innerHTML = `
        <div class="empty-state">
          <h3>No data yet</h3>
          <p>Make some API requests through the proxy to see stats here.</p>
          <p style="margin-top: 12px; font-family: monospace; font-size: 0.85rem; color: #666;">
            Proxy: http://localhost:8008
          </p>
        </div>
      `;
      return;
    }

    content.innerHTML = renderDashboard(data, dlpStats);

    // Create charts after DOM is updated
    setTimeout(() => {
      if (data.models.length > 0) {
        createModelsChart(document.getElementById('models-chart'), data.models);
      }
      if (data.recent_requests.length > 0) {
        createTokenChart(document.getElementById('token-chart'), data.recent_requests);
      }
      if (data.latency_points.length > 0) {
        createLatencyChart(document.getElementById('latency-chart'), data.latency_points);
      }
      if (dlpStats && dlpStats.detections_by_pattern.length > 0) {
        createDlpChart(document.getElementById('dlp-chart'), dlpStats.detections_by_pattern);
      }
    }, 0);

  } catch (error) {
    content.innerHTML = `
      <div class="empty-state">
        <h3>Error loading stats</h3>
        <p>${error}</p>
      </div>
    `;
  }
}

// Load available backends
export async function loadBackends() {
  try {
    const backends = await invoke('get_backends');
    const select = document.getElementById('backend-select');
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

// Initialize backend filter
export function initBackendFilter() {
  const select = document.getElementById('backend-select');
  select.addEventListener('change', () => {
    setCurrentBackend(select.value);
    loadDashboard();
  });
}

// Initialize time filter
export function initTimeFilter() {
  const select = document.getElementById('time-select');
  select.addEventListener('change', () => {
    setCurrentTimeRange(select.value);
    loadDashboard();
  });
}
