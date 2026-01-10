import { charts, setCharts, colors, formatNumber, formatLatency, shortenModel } from './utils.js';

// Extended color palette for DLP chart
const dlpColors = [
  colors.primary,    // Indigo
  colors.secondary,  // Green
  colors.warning,    // Amber
  colors.pink,       // Pink
  colors.blue,       // Blue
  '#8b5cf6',         // Purple
  '#14b8a6',         // Teal
  '#f97316',         // Orange
  '#ef4444',         // Red
  '#84cc16',         // Lime
];

// Destroy existing charts
export function destroyCharts() {
  Object.values(charts).forEach(chart => chart.destroy());
  setCharts({});
}

// Create Models Chart (Horizontal Bar)
export function createModelsChart(container, models) {
  const ctx = document.createElement('canvas');
  container.appendChild(ctx);

  const data = models.slice(0, 5); // Top 5 models
  const labels = data.map(m => shortenModel(m.model));
  const values = data.map(m => m.count);

  const newCharts = { ...charts };
  newCharts.models = new Chart(ctx, {
    type: 'bar',
    data: {
      labels,
      datasets: [{
        data: values,
        backgroundColor: [colors.primary, colors.secondary, colors.warning, colors.pink, colors.blue],
        borderRadius: 6,
        barThickness: 24,
      }]
    },
    options: {
      indexAxis: 'y',
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: { display: false }
      },
      scales: {
        x: {
          grid: { display: false },
          ticks: { font: { size: 11 } }
        },
        y: {
          grid: { display: false },
          ticks: { font: { size: 11 } }
        }
      }
    }
  });
  setCharts(newCharts);
}

// Create Token Usage Chart (Stacked Bar per request)
export function createTokenChart(container, requests) {
  const ctx = document.createElement('canvas');
  container.appendChild(ctx);

  // Reverse to show oldest first (left to right)
  const data = [...requests].reverse().slice(-15);
  const labels = data.map((_, i) => `#${i + 1}`);

  const newCharts = { ...charts };
  newCharts.tokens = new Chart(ctx, {
    type: 'bar',
    data: {
      labels,
      datasets: [
        {
          label: 'Input',
          data: data.map(r => r.input_tokens),
          backgroundColor: colors.primary,
          borderRadius: 4,
        },
        {
          label: 'Output',
          data: data.map(r => r.output_tokens),
          backgroundColor: colors.secondary,
          borderRadius: 4,
        },
        {
          label: 'Cache Read',
          data: data.map(r => r.cache_read_tokens),
          backgroundColor: colors.warning,
          borderRadius: 4,
        },
      ]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: {
          position: 'top',
          labels: {
            boxWidth: 12,
            padding: 16,
            font: { size: 11 }
          }
        }
      },
      scales: {
        x: {
          stacked: true,
          grid: { display: false },
          ticks: { font: { size: 10 } }
        },
        y: {
          stacked: true,
          grid: { color: '#f0f0f0' },
          ticks: {
            font: { size: 10 },
            callback: v => formatNumber(v)
          }
        }
      }
    }
  });
  setCharts(newCharts);
}

// Create Latency Chart (Line)
export function createLatencyChart(container, latencyPoints) {
  const ctx = document.createElement('canvas');
  container.appendChild(ctx);

  // Reverse to show oldest first
  const data = [...latencyPoints].reverse();
  const labels = data.map((_, i) => i + 1);
  const values = data.map(p => p.latency_ms);

  const newCharts = { ...charts };
  newCharts.latency = new Chart(ctx, {
    type: 'line',
    data: {
      labels,
      datasets: [{
        label: 'Latency (ms)',
        data: values,
        borderColor: colors.primary,
        backgroundColor: 'rgba(99, 102, 241, 0.1)',
        fill: true,
        tension: 0.3,
        pointRadius: 2,
        pointHoverRadius: 4,
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: { display: false }
      },
      scales: {
        x: {
          display: false,
        },
        y: {
          grid: { color: '#f0f0f0' },
          ticks: {
            font: { size: 10 },
            callback: v => formatLatency(v)
          }
        }
      }
    }
  });
  setCharts(newCharts);
}

// Create Tool Calls Chart (Horizontal Bar) - kept for backwards compatibility
export function createToolCallsChart(container, toolCallStats) {
  const ctx = document.createElement('canvas');
  container.appendChild(ctx);

  const data = toolCallStats.slice(0, 10); // Top 10 tools
  const labels = data.map(t => t.tool_name);
  const values = data.map(t => t.count);

  const newCharts = { ...charts };
  newCharts.toolCalls = new Chart(ctx, {
    type: 'bar',
    data: {
      labels,
      datasets: [{
        data: values,
        backgroundColor: dlpColors.slice(0, data.length),
        borderRadius: 6,
        barThickness: 20,
      }]
    },
    options: {
      indexAxis: 'y',
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: { display: false }
      },
      scales: {
        x: {
          grid: { display: false },
          ticks: { font: { size: 11 } }
        },
        y: {
          grid: { display: false },
          ticks: { font: { size: 11 } }
        }
      }
    }
  });
  setCharts(newCharts);
}

// Create Tool Insights Chart (Nested Doughnut / Sunburst)
// Inner ring: tools, Outer ring: targets aligned with each tool's arc
export function createToolInsightsChart(container, insights) {
  const ctx = document.createElement('canvas');
  container.appendChild(ctx);

  const { tools } = insights;

  // Inner ring: tools
  const innerLabels = tools.map(t => t.tool_name);
  const innerValues = tools.map(t => t.count);
  const innerColors = tools.map((_, i) => dlpColors[i % dlpColors.length]);

  // Outer ring: targets aligned with parent tool's arc
  // Each tool's targets must sum to that tool's count to align properly
  const outerLabels = [];
  const outerValues = [];
  const outerColors = [];
  const outerMeta = []; // Store tool name for tooltip

  tools.forEach((tool, toolIndex) => {
    const baseColor = dlpColors[toolIndex % dlpColors.length];

    if (tool.targets.length === 0) {
      // No targets extracted - show single segment labeled "other"
      outerLabels.push('other');
      outerValues.push(tool.count);
      outerColors.push(baseColor + 'AA');
      outerMeta.push({ tool: tool.tool_name, target: 'other' });
    } else {
      // Add each target
      let targetSum = 0;
      tool.targets.forEach((target, targetIndex) => {
        outerLabels.push(target.target);
        outerValues.push(target.count);
        // Vary opacity slightly for each target
        const opacity = Math.max(60, 99 - targetIndex * 15).toString(16).padStart(2, '0');
        outerColors.push(baseColor + opacity);
        outerMeta.push({ tool: tool.tool_name, target: target.target });
        targetSum += target.count;
      });

      // Add "other" segment for remaining count
      const remaining = tool.count - targetSum;
      if (remaining > 0) {
        outerLabels.push('other');
        outerValues.push(remaining);
        outerColors.push(baseColor + '40');
        outerMeta.push({ tool: tool.tool_name, target: 'other' });
      }
    }
  });

  const newCharts = { ...charts };
  newCharts.toolInsights = new Chart(ctx, {
    type: 'doughnut',
    data: {
      datasets: [
        {
          // Outer ring - targets (larger circle)
          label: 'Targets',
          data: outerValues,
          backgroundColor: outerColors,
          borderWidth: 1,
          borderColor: '#fff',
        },
        {
          // Inner ring - tools (smaller circle)
          label: 'Tools',
          data: innerValues,
          backgroundColor: innerColors,
          borderWidth: 2,
          borderColor: '#fff',
        }
      ]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      cutout: '30%',
      plugins: {
        legend: {
          display: true,
          position: 'right',
          labels: {
            boxWidth: 12,
            padding: 8,
            font: { size: 11 },
            color: '#e0e0e0',
            generateLabels: () => {
              // Only show tools in legend
              return tools.map((tool, i) => ({
                text: `${tool.tool_name} (${tool.count})`,
                fillStyle: dlpColors[i % dlpColors.length],
                strokeStyle: '#fff',
                fontColor: '#e0e0e0',
                lineWidth: 1,
                index: i
              }));
            }
          }
        },
        tooltip: {
          callbacks: {
            label: (context) => {
              const datasetIndex = context.datasetIndex;
              const index = context.dataIndex;
              if (datasetIndex === 1) {
                // Inner ring - tool
                return `${tools[index].tool_name}: ${tools[index].count} calls`;
              } else {
                // Outer ring - target
                const meta = outerMeta[index];
                return `${meta.tool} → ${meta.target}: ${outerValues[index]}`;
              }
            }
          }
        }
      }
    }
  });
  setCharts(newCharts);
}

// Create DLP Detections Chart (Doughnut)
export function createDlpChart(container, detectionsByPattern) {
  const ctx = document.createElement('canvas');
  container.appendChild(ctx);

  const labels = detectionsByPattern.map(p => p.pattern_name);
  const values = detectionsByPattern.map(p => p.count);
  const backgroundColors = detectionsByPattern.map((_, i) => dlpColors[i % dlpColors.length]);

  const newCharts = { ...charts };
  newCharts.dlp = new Chart(ctx, {
    type: 'doughnut',
    data: {
      labels,
      datasets: [{
        data: values,
        backgroundColor: backgroundColors,
        borderWidth: 2,
        borderColor: '#fff',
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: {
          position: 'right',
          labels: {
            boxWidth: 14,
            padding: 12,
            font: { size: 12 },
            generateLabels: (chart) => {
              const data = chart.data;
              return data.labels.map((label, i) => ({
                text: `${label} (${data.datasets[0].data[i]})`,
                fillStyle: data.datasets[0].backgroundColor[i],
                strokeStyle: '#fff',
                lineWidth: 2,
                index: i
              }));
            }
          }
        },
        tooltip: {
          callbacks: {
            label: (context) => {
              const total = context.dataset.data.reduce((a, b) => a + b, 0);
              const value = context.parsed;
              const pct = Math.round((value / total) * 100);
              return `${context.label}: ${value} (${pct}%)`;
            }
          }
        }
      }
    }
  });
  setCharts(newCharts);
}
