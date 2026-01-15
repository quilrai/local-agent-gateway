// Main entry point - imports and initializes all modules

import { initTabs } from './utils.js';
import { loadDashboard, loadBackends, initBackendFilter, initTimeFilter } from './dashboard.js';
import {
  loadMessageLogs,
  loadLogsBackends,
  loadLogsModels,
  initLogsBackendFilter,
  initLogsModelFilter,
  initLogsDlpFilter,
  initLogsTimeFilter,
  initLogsSearch,
  initLogsExport
} from './logs.js';
import { initSettings } from './settings.js';
import { initBackends } from './backends.js';
import { initHowTo } from './howto.js';

// Initialize app
window.addEventListener('DOMContentLoaded', () => {
  // Initialize Lucide icons
  lucide.createIcons();

  // Initialize navigation
  initTabs();

  // Initialize dashboard
  initTimeFilter();
  initBackendFilter();
  loadBackends();
  loadDashboard();

  // Initialize logs
  initLogsTimeFilter();
  initLogsBackendFilter();
  initLogsModelFilter();
  initLogsDlpFilter();
  initLogsSearch();
  initLogsExport();
  loadLogsBackends();
  loadLogsModels();

  // Initialize settings
  initSettings();

  // Initialize custom backends
  initBackends();

  // Initialize how-to
  initHowTo();

  // Refresh buttons - also refresh backends list
  document.getElementById('refresh-btn').addEventListener('click', () => {
    loadBackends();
    loadDashboard();
  });
  document.getElementById('logs-refresh-btn').addEventListener('click', () => {
    loadLogsBackends();
    loadMessageLogs();
  });

  // Load logs when tab is clicked
  document.querySelector('[data-tab="logs"]').addEventListener('click', () => {
    loadMessageLogs();
    loadLogsModels();
  });
});
