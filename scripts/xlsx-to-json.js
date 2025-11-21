#!/usr/bin/env node
// Usage: node scripts/xlsx-to-json.js backend/listening_history.xlsx frontend/public/listening_history.json
// Converts first sheet of the XLSX into compact JSON (no pretty-print).

const fs = require('fs');
const path = require('path');
const xlsx = require('xlsx');

function fail(msg, code = 1) {
  console.error(msg);
  process.exit(code);
}

if (process.argv.length < 4) {
  fail('Usage: node scripts/xlsx-to-json.js <input.xlsx> <output.json>', 2);
}

const input = process.argv[2];
const output = process.argv[3];

if (!fs.existsSync(input)) fail('Input file not found: ' + input, 2);

try {
  const wb = xlsx.readFile(input, { cellDates: true, dateNF: 'yyyy-mm-dd HH:MM:SS' });
  const firstSheet = wb.SheetNames[0];
  if (!firstSheet) fail('No sheets found in workbook.', 3);

  const ws = wb.Sheets[firstSheet];
  const rows = xlsx.utils.sheet_to_json(ws, { defval: null, raw: false });

  const outDir = path.dirname(output);
  if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });

  fs.writeFileSync(output, JSON.stringify(rows), 'utf8');
  console.log(`Wrote ${rows.length} records to ${output}`);
  process.exit(0);
} catch (err) {
  console.error('Conversion error:', err);
  process.exit(3);
}
