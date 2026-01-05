import { invoke, getCurrentPort } from './utils.js';

// Get instructions for each tool
function getToolInstructions(tool) {
  const port = getCurrentPort();

  const instructions = {
    'claude-code': {
      title: 'Claude Code CLI',
      content: `
        <p>Run Claude Code with the proxy inline:</p>
        <code>ANTHROPIC_BASE_URL="http://localhost:${port}/claude" claude</code>

        <p style="margin-top: 24px;"><strong>Or set globally:</strong></p>

        <div class="shell-tabs">
          <button class="shell-tab active" data-shell="bash">Bash</button>
          <button class="shell-tab" data-shell="zsh">Zsh</button>
          <button class="shell-tab" data-shell="fish">Fish</button>
        </div>

        <div class="shell-tab-content active" data-shell="bash">
          <p class="shell-config-path">~/.bashrc</p>
          <code>export ANTHROPIC_BASE_URL="http://localhost:${port}/claude"</code>
          <button class="btn btn-primary shell-action-btn" data-shell="bash" data-action="set">Set</button>
        </div>

        <div class="shell-tab-content" data-shell="zsh">
          <p class="shell-config-path">~/.zshrc</p>
          <code>export ANTHROPIC_BASE_URL="http://localhost:${port}/claude"</code>
          <button class="btn btn-primary shell-action-btn" data-shell="zsh" data-action="set">Set</button>
        </div>

        <div class="shell-tab-content" data-shell="fish">
          <p class="shell-config-path">Universal variable (persists automatically)</p>
          <code>set -Ux ANTHROPIC_BASE_URL "http://localhost:${port}/claude"</code>
          <button class="btn btn-primary shell-action-btn" data-shell="fish" data-action="set">Set</button>
        </div>

        <div id="shell-set-status" class="shell-set-status"></div>
      `
    },
    'cursor': {
      title: 'Cursor',
      content: `
        <p>Cursor uses hooks for data protection integration. Click the button below to install or remove the hooks.</p>

        <div class="cursor-hooks-section">
          <div class="cursor-hooks-status">
            <span class="status-indicator" id="cursor-status-indicator"></span>
            <span id="cursor-status-text">Checking status...</span>
          </div>
          <button id="cursor-hooks-btn" class="btn btn-primary cursor-hooks-btn" disabled>
            Install Hooks
          </button>
        </div>

        <div id="cursor-action-status" class="shell-set-status"></div>

        <div class="cursor-info" style="margin-top: 24px;">
          <h4>What this does:</h4>
          <ul>
            <li>Creates a hook script at <code>~/.cursor/quilr-cursor-hooks.sh</code></li>
            <li>Configures <code>~/.cursor/hooks.json</code> to use the hook</li>
            <li>Intercepts prompts and file reads to check for sensitive data</li>
            <li>Blocks requests containing detected patterns (API keys, custom patterns)</li>
          </ul>
        </div>

        <div class="cursor-info" style="margin-top: 16px;">
          <h4>Hooks enabled:</h4>
          <ul>
            <li><strong>beforeSubmitPrompt</strong> - Check prompts before sending</li>
            <li><strong>beforeReadFile</strong> - Check file contents before agent reads</li>
            <li><strong>beforeTabFileRead</strong> - Check files for Tab completions</li>
            <li><strong>afterAgentResponse</strong> - Log agent responses</li>
            <li><strong>afterAgentThought</strong> - Log thinking process</li>
            <li><strong>afterTabFileEdit</strong> - Log Tab edits</li>
          </ul>
        </div>
      `
    },
    'codex': {
      title: 'Codex CLI',
      content: `
        <p>Run Codex CLI with the proxy inline:</p>
        <code>OPENAI_BASE_URL="http://localhost:${port}/codex" codex</code>

        <p style="margin-top: 24px;"><strong>Or set globally:</strong></p>

        <div class="shell-tabs">
          <button class="shell-tab active" data-shell="bash">Bash</button>
          <button class="shell-tab" data-shell="zsh">Zsh</button>
          <button class="shell-tab" data-shell="fish">Fish</button>
        </div>

        <div class="shell-tab-content active" data-shell="bash">
          <p class="shell-config-path">~/.bashrc</p>
          <code>export OPENAI_BASE_URL="http://localhost:${port}/codex"</code>
          <button class="btn btn-primary shell-action-btn" data-shell="bash" data-action="set">Set</button>
        </div>

        <div class="shell-tab-content" data-shell="zsh">
          <p class="shell-config-path">~/.zshrc</p>
          <code>export OPENAI_BASE_URL="http://localhost:${port}/codex"</code>
          <button class="btn btn-primary shell-action-btn" data-shell="zsh" data-action="set">Set</button>
        </div>

        <div class="shell-tab-content" data-shell="fish">
          <p class="shell-config-path">Universal variable (persists automatically)</p>
          <code>set -Ux OPENAI_BASE_URL "http://localhost:${port}/codex"</code>
          <button class="btn btn-primary shell-action-btn" data-shell="fish" data-action="set">Set</button>
        </div>

        <div id="shell-set-status" class="shell-set-status"></div>
      `
    },
  };

  return instructions[tool] || { title: 'Unknown', content: '<p>No instructions available.</p>' };
}

// Update a button to show Set or Remove
function updateButtonState(btn, isSet) {
  if (isSet) {
    btn.textContent = 'Remove';
    btn.dataset.action = 'remove';
    btn.classList.remove('btn-primary');
    btn.classList.add('btn-danger');
  } else {
    btn.textContent = 'Set';
    btn.dataset.action = 'set';
    btn.classList.remove('btn-danger');
    btn.classList.add('btn-primary');
  }
}

// Track currently active tool
let currentTool = 'claude-code';

// Check shell env status and update button states
async function updateShellButtonStates(tool) {
  const shells = ['bash', 'zsh', 'fish'];

  for (const shell of shells) {
    try {
      const isSet = await invoke('check_shell_env', { shell, tool });
      const btn = document.querySelector(`.shell-action-btn[data-shell="${shell}"]`);
      const tab = document.querySelector(`.shell-tab[data-shell="${shell}"]`);

      if (btn) {
        updateButtonState(btn, isSet);
      }

      // Add/remove indicator on tab
      if (tab) {
        if (isSet) {
          tab.classList.add('is-set');
        } else {
          tab.classList.remove('is-set');
        }
      }
    } catch (error) {
      // Shell might not be installed, leave button as "Set"
      console.log(`Could not check ${shell}: ${error}`);
    }
  }
}

// Handle shell action (set or remove)
async function handleShellAction(btn) {
  const shell = btn.dataset.shell;
  const action = btn.dataset.action;
  const statusDiv = document.getElementById('shell-set-status');
  const tool = currentTool;

  btn.disabled = true;
  btn.textContent = action === 'set' ? 'Setting...' : 'Removing...';

  try {
    let result;
    if (action === 'set') {
      result = await invoke('set_shell_env', { shell, tool });
    } else {
      result = await invoke('remove_shell_env', { shell, tool });
    }

    // Show success
    btn.textContent = 'Done!';
    btn.classList.remove('btn-primary', 'btn-danger');
    btn.classList.add('btn-success');

    if (statusDiv) {
      statusDiv.textContent = result;
      statusDiv.className = 'shell-set-status show success';
    }

    // Update button and tab state after success
    setTimeout(() => {
      btn.classList.remove('btn-success');
      btn.disabled = false;
      // Toggle the action
      const newIsSet = action === 'set';
      updateButtonState(btn, newIsSet);

      // Update tab indicator
      const tab = document.querySelector(`.shell-tab[data-shell="${shell}"]`);
      if (tab) {
        if (newIsSet) {
          tab.classList.add('is-set');
        } else {
          tab.classList.remove('is-set');
        }
      }
    }, 1500);
  } catch (error) {
    btn.textContent = 'Failed';
    btn.classList.remove('btn-primary', 'btn-danger');
    btn.classList.add('btn-error');

    if (statusDiv) {
      statusDiv.textContent = error;
      statusDiv.className = 'shell-set-status show error';
    }

    // Reset button after 3 seconds
    setTimeout(() => {
      btn.classList.remove('btn-error');
      btn.disabled = false;
      updateButtonState(btn, action === 'remove'); // Restore original state
    }, 3000);
  }
}

// Check Cursor hooks installation status
async function checkCursorHooksStatus() {
  const statusIndicator = document.getElementById('cursor-status-indicator');
  const statusText = document.getElementById('cursor-status-text');
  const btn = document.getElementById('cursor-hooks-btn');

  if (!statusIndicator || !statusText || !btn) return;

  try {
    const isInstalled = await invoke('check_cursor_hooks_installed');

    if (isInstalled) {
      statusIndicator.className = 'status-indicator installed';
      statusText.textContent = 'Hooks installed';
      btn.textContent = 'Remove Hooks';
      btn.dataset.action = 'remove';
      btn.classList.remove('btn-primary');
      btn.classList.add('btn-danger');
    } else {
      statusIndicator.className = 'status-indicator not-installed';
      statusText.textContent = 'Not installed';
      btn.textContent = 'Install Hooks';
      btn.dataset.action = 'install';
      btn.classList.remove('btn-danger');
      btn.classList.add('btn-primary');
    }
    btn.disabled = false;
  } catch (error) {
    statusIndicator.className = 'status-indicator error';
    statusText.textContent = 'Error checking status';
    btn.disabled = true;
    console.error('Failed to check Cursor hooks status:', error);
  }
}

// Handle Cursor hooks install/uninstall
async function handleCursorHooksAction(btn) {
  const action = btn.dataset.action;
  const statusDiv = document.getElementById('cursor-action-status');

  btn.disabled = true;
  btn.textContent = action === 'install' ? 'Installing...' : 'Removing...';

  try {
    let result;
    if (action === 'install') {
      result = await invoke('install_cursor_hooks');
    } else {
      result = await invoke('uninstall_cursor_hooks');
    }

    // Show success
    btn.textContent = 'Done!';
    btn.classList.remove('btn-primary', 'btn-danger');
    btn.classList.add('btn-success');

    if (statusDiv) {
      statusDiv.textContent = result;
      statusDiv.className = 'shell-set-status show success';
    }

    // Update status after success
    setTimeout(async () => {
      btn.classList.remove('btn-success');
      await checkCursorHooksStatus();
    }, 1500);
  } catch (error) {
    btn.textContent = 'Failed';
    btn.classList.remove('btn-primary', 'btn-danger');
    btn.classList.add('btn-error');

    if (statusDiv) {
      statusDiv.textContent = error;
      statusDiv.className = 'shell-set-status show error';
    }

    // Reset button after 3 seconds
    setTimeout(async () => {
      btn.classList.remove('btn-error');
      await checkCursorHooksStatus();
    }, 3000);
  }
}

// Show instructions for selected tool
async function showToolInstructions(tool) {
  const instructionsDiv = document.getElementById('howto-instructions');
  const buttons = document.querySelectorAll('.howto-tool-btn');

  // Update active button
  buttons.forEach(btn => {
    if (btn.dataset.tool === tool) {
      btn.classList.add('active');
    } else {
      btn.classList.remove('active');
    }
  });

  // Show instructions
  const info = getToolInstructions(tool);
  instructionsDiv.innerHTML = `
    <h3>${info.title}</h3>
    ${info.content}
  `;

  // Add click handlers for shell action buttons
  instructionsDiv.querySelectorAll('.shell-action-btn').forEach(btn => {
    btn.addEventListener('click', () => handleShellAction(btn));
  });

  // Add click handlers for shell tabs
  instructionsDiv.querySelectorAll('.shell-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      const shell = tab.dataset.shell;

      // Update active tab
      instructionsDiv.querySelectorAll('.shell-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');

      // Update active content
      instructionsDiv.querySelectorAll('.shell-tab-content').forEach(c => c.classList.remove('active'));
      instructionsDiv.querySelector(`.shell-tab-content[data-shell="${shell}"]`).classList.add('active');
    });
  });

  // Check and update button states for tools with shell buttons
  if (tool === 'claude-code' || tool === 'codex') {
    currentTool = tool;
    await updateShellButtonStates(tool);
  }

  // Handle Cursor hooks
  if (tool === 'cursor') {
    await checkCursorHooksStatus();

    const cursorBtn = document.getElementById('cursor-hooks-btn');
    if (cursorBtn) {
      cursorBtn.addEventListener('click', () => handleCursorHooksAction(cursorBtn));
    }
  }
}

// Initialize How to use tab
export function initHowTo() {
  const buttons = document.querySelectorAll('.howto-tool-btn');
  buttons.forEach(btn => {
    btn.addEventListener('click', () => {
      showToolInstructions(btn.dataset.tool);
    });
  });
}
