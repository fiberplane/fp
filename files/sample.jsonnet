local fiberplane = import 'fiberplane.libsonnet';
local notebook = fiberplane.notebook;
local cell = fiberplane.cell;

local parse_alert(alert) =
  local base = std.format('This alert was triggered at: %s, more info: %s', [alert.startsAt, alert.generatorURL]);
  local content = if std.objectHas(alert.labels, 'runbook') then base + '. See runbook: ' + alert.labels.runbook else base;

  cell.text(content);

function(model)
  notebook
  .new('Some title')
  .addCell(cell.heading('Alert!'))
  .addCells([parse_alert(alert) for alert in model.alerts])
