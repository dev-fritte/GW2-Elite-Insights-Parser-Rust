// Reads the mechanic declarations of the C# encounter logics and writes
// docs/phase7/mechanic-filters.tsv: for each mechanic whose only addition is one checker lambda of
// a known shape, the filter that lambda stands for. Lambdas are code, so the reflection dump
// (`GW2EIParserReference --dump-mechanics`) cannot list them; Gen-Mechanics.ps1 joins both by
// logic class, mechanic type and short name.
// Usage: node tools/gen/Extract-Checkers.js
'use strict';
const fs = require('fs');
const path = require('path');
const repo = path.resolve(__dirname, '..', '..');
const root = path.join(repo, 'GW2EI.Library/GW2EI.Services/GW2EIEvtcParser/LogLogic');
const out = path.join(repo, 'docs/phase7/mechanic-filters.tsv');

function walk(d, o = []) {
  for (const f of fs.readdirSync(d)) {
    const p = path.join(d, f);
    if (fs.statSync(p).isDirectory()) walk(p, o);
    else if (p.endsWith('.cs')) o.push(p);
  }
  return o;
}

// index after the parenthesis that closes the one at s[i], skipping string and char literals
function balanced(s, i) {
  let depth = 0, inStr = false, quote = '';
  for (let j = i; j < s.length; j++) {
    const c = s[j];
    if (inStr) {
      if (c === String.fromCharCode(92) && quote === '"') { j++; continue; }
      if (c === quote) inStr = false;
      continue;
    }
    if (c === '"' || c === "'") { inStr = true; quote = c; continue; }
    if (c === '(') depth++;
    else if (c === ')' && --depth === 0) return j + 1;
  }
  return -1;
}

function classify(chain) {
  const checkers = chain.filter(x => x[0] === 'UsingChecker' || x[0] === 'UsingBuffChecker');
  if (checkers.length !== 1) return null;
  const [name, a] = checkers[0];
  let m;
  if (name === 'UsingBuffChecker' && a === 'Stability, false') return 'nostab';
  if ((m = /^\(\w+,\s*log\)\s*=>\s*\w+\.ActualDuration\s*(>=|<=|>|<)\s*(\d+)$/.exec(a))) {
    return { '>=': 'dur>=', '<=': 'dur<=', '>': 'dur>', '<': 'dur<' }[m[1]] + m[2];
  }
  if ((m = /^\(\w+,\s*log\)\s*=>\s*(!?)\w+\.IsInterrupted$/.exec(a))) return m[1] ? 'notinterrupted' : 'interrupted';
  if (/^\(\w+,\s*log\)\s*=>\s*\w+\.HealthDamage > 0$/.test(a)) return 'damage';
  if (/^\(\w+,\s*log\)\s*=>\s*\w+\.HasKilled$/.test(a)) return 'killed';
  if (/^\(\w+,\s*log\)\s*=>\s*\w+\.HasDowned$/.test(a)) return 'downed';
  if (/^\(\w+,\s*log\)\s*=>\s*\w+\.HasDowned \|\| \w+\.HasKilled$/.test(a)) return 'downedorkilled';
  return null;
}

const seen = new Map();
for (const file of walk(root)) {
  const s = fs.readFileSync(file, 'utf8').replace(/\r/g, '');
  const re = /new ([A-Za-z]+Mechanic)(?:<[^>]*>)?\(/g;
  let m;
  while ((m = re.exec(s))) {
    const open = m.index + m[0].length - 1;
    const end = balanced(s, open);
    if (end < 0) continue;
    const args = s.slice(open + 1, end - 1);
    const short = /\bnew\s*\(\s*"([^"]*)"/.exec(args);
    if (!short) continue;
    const chain = [];
    for (let k = end; ;) {
      const mm = /^\s*\.(\w+)\(/.exec(s.slice(k, k + 80));
      if (!mm) break;
      const o2 = k + mm[0].length - 1;
      const e2 = balanced(s, o2);
      if (e2 < 0) break;
      chain.push([mm[1], s.slice(o2 + 1, e2 - 1).replace(/\s+/g, ' ').trim()]);
      k = e2;
    }
    // only a checker and build ranges may be added
    if (chain.some(x => !/^Using(Checker|BuffChecker)$/.test(x[0]) && x[0] !== 'WithBuilds')) continue;
    const filter = classify(chain);
    const key = [path.basename(file, '.cs'), m[1], short[1]].join('\t');
    if (!seen.has(key)) seen.set(key, new Set());
    seen.get(key).add(filter || '');
  }
}
const lines = ['Logic\tType\tShort\tFilter'];
for (const [key, filters] of [...seen].sort((a, b) => (a[0] < b[0] ? -1 : 1))) {
  // the same name declared twice with different shapes cannot be told apart
  if (filters.size === 1 && [...filters][0]) lines.push(key + '\t' + [...filters][0]);
}
fs.writeFileSync(out, lines.join('\n') + '\n');
console.log(lines.length - 1 + ' filters');
