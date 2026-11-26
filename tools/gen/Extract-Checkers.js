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

// numeric values of the TargetID enum; names defined by other constants are left out
const targetIds = new Map();
{
  const src = fs.readFileSync(path.join(root, '..', 'ParserHelpers/IDs/SpeciesIDs.cs'), 'utf8').replace(/\r/g, '');
  const start = src.indexOf('enum TargetID');
  const end = src.indexOf('\n    }', start);
  const consts = new Map();
  for (const m of src.matchAll(/const int ([A-Za-z0-9_]+) *= *(-?[0-9]+);/g)) consts.set(m[1], m[2]);
  for (const m of src.slice(start, end).matchAll(/^ *([A-Za-z0-9_]+) *= *(-?[0-9]+|SpeciesIDs[.][A-Za-z0-9_]+) *,/gm)) {
    const v = m[2].startsWith('SpeciesIDs.') ? consts.get(m[2].slice(11)) : m[2];
    if (v !== undefined) targetIds.set(m[1], v);
  }
}

// numeric values of the skill and buff constants
const skillIds = new Map();
{
  const src = fs.readFileSync(path.join(root, '..', 'ParserHelpers/IDs/SkillIDs.cs'), 'utf8').replace(/\r/g, '');
  for (const m of src.matchAll(/const long (\w+)\s*=\s*(\d+);/g)) skillIds.set(m[1], m[2]);
}

// ids of the species a checker lists, or null if one is not a plain number
function speciesIds(list) {
  const ids = [];
  for (const m of list.matchAll(/TargetID\.(\w+)/g)) {
    if (!targetIds.has(m[1])) return null;
    ids.push(targetIds.get(m[1]));
  }
  const rest = list.replace(/TargetID\.\w+/g, '').replace(/new List<TargetID>|\(int\)|[\[\]{}(),\s]/g, '');
  return ids.length && !rest ? ids : null;
}

function classify(chain) {
  const checkers = chain.filter(x => x[0] === 'UsingChecker' || x[0] === 'UsingBuffChecker');
  if (checkers.length !== 1) return null;
  const [name, a] = checkers[0];
  let m;
  // the remover is a player (or a minion of one), or the removed from agent is one
  if ((m = /^[(][A-Za-z0-9_]+, *log[)] *=> *[A-Za-z0-9_]+[.](CreditedBy|By|To)[.]IsPlayer$/.exec(a))) return m[1] === 'To' ? 'toplayer' : 'byplayer';
  // a direct hit on an agent of one or two species
  if ((m = /^[(][A-Za-z0-9_]+, *log[)] *=> *[(]?[A-Za-z0-9_]+[.]To[.]IsSpecies[(]TargetID[.]([A-Za-z0-9_]+)[)](?: *[|][|] *[A-Za-z0-9_]+[.]To[.]IsSpecies[(]TargetID[.]([A-Za-z0-9_]+)[)])?[)]? *&& *[A-Za-z0-9_]+ is DirectHealthDamageEvent$/.exec(a))) {
    const ids = [m[1], m[2]].filter(Boolean).map(n => targetIds.get(n));
    if (ids.every(Boolean)) return 'species:to=' + ids.join(',') + '&direct';
  }
  // the target has (or has not) a buff when the event happens, possibly within a window
  if ((m = /^[(][A-Za-z0-9_]+, *log[)] *=> *(!?) *[A-Za-z0-9_]+[.]To[.]HasBuff[(]log, *([A-Za-z0-9_]+), *[A-Za-z0-9_]+[.]Time( *- *(?:ParserHelper[.])?ServerDelayConstant)?(, *(?:ParserHelper[.])?ServerDelayConstant)?[)]$/.exec(a))) {
    if (skillIds.has(m[2])) return 'buff:' + skillIds.get(m[2]) + ',' + (m[1] ? 'no' : 'has') + ',' + (m[3] ? 10 : 0) + ',' + (m[4] ? 10 : 0);
  }
  // the target is of the species and has (or has not) a buff when the event happens
  if ((m = /^\(\w+,\s*log\)\s*=>\s*\w+\.To\.IsSpecies\(TargetID\.(\w+)\)\s*(?:&&|&)\s*(!?)\s*\w+\.To\.HasBuff\(log,\s*(\w+),\s*\w+\.Time(?:\s*-\s*(?:ParserHelper\.)?ServerDelayConstant)?\)$/.exec(a))) {
    if (targetIds.has(m[1]) && skillIds.has(m[3])) {
      const delay = /ServerDelayConstant/.test(a) ? 10 : 0;
      return 'speciesbuff:' + targetIds.get(m[1]) + ',' + skillIds.get(m[3]) + ',' + (m[2] ? 'no' : 'has') + ',' + delay;
    }
  }
  // the species of an agent of the event: the target ('to') or the source ('by')
  if ((m = /^\(\w+,\s*log\)\s*=>\s*\w+\.(To|By|From|CreditedBy|CreditedFrom|Src)\.Is(?:Any)?Species\((.*)\)$/.exec(a))) {
    const ids = speciesIds(m[2]);
    if (ids) return 'species:' + (m[1] === 'To' ? 'to' : 'by') + '=' + ids.join(',');
  }
  if (name === 'UsingBuffChecker' && a === 'Stability, false') return 'nostab';
  if ((m = /^\(\w+,\s*log\)\s*=>\s*\w+\.ActualDuration\s*(>=|<=|>|<)\s*(\d+)$/.exec(a))) {
    return { '>=': 'dur>=', '<=': 'dur<=', '>': 'dur>', '<': 'dur<' }[m[1]] + m[2];
  }
  if ((m = /^\(\w+,\s*log\)\s*=>\s*(!?)\w+\.IsInterrupted$/.exec(a))) return m[1] ? 'notinterrupted' : 'interrupted';
  if (/^\(\w+,\s*log\)\s*=>\s*\w+ is AnimatedCastEvent (\w+) && \1\.IsInterrupted$/.test(a)) return 'interrupted';
  if ((m = /^\(\w+,\s*log\)\s*=>\s*\w+\.AppliedDuration == (\d+)$/.exec(a))) return 'applied=' + m[1];
  if (/^\(\w+,\s*log\)\s*=>\s*\w+\.HealthDamage > 0$/.test(a)) return 'damage';
  if ((m = /^\(\w+,\s*log\)\s*=>\s*(!?)\w+\.Lost$/.exec(a))) return m[1] ? 'kept' : 'lost';
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
    // only a checker, build ranges, the ignoring of the mechanic itself and sub mechanics that
    // split the events by stability may be added
    if (chain.some(x => !/^Using(Checker|BuffChecker|Ignored)$/.test(x[0]) && !['WithBuilds', 'WithStabilitySubMechanic'].includes(x[0]))) continue;
    for (const [name, a] of chain) {
      if (name !== 'WithStabilitySubMechanic') continue;
      const sub = /^new SubMechanic[(].*? new *[(] *"([^"]*)".*, *(true|false)$/.exec(a);
      if (!sub) continue;
      const subKey = [path.basename(file, '.cs'), 'SubMechanic', sub[1]].join('	');
      if (!seen.has(subKey)) seen.set(subKey, new Set());
      seen.get(subKey).add('sub:' + short[1] + ':' + (sub[2] === 'true' ? 'stab' : 'nostab'));
    }
    const filter = classify(chain);
    const key = [path.basename(file, '.cs'), m[1], short[1]].join('\t');
    if (!seen.has(key)) seen.set(key, new Set());
    if (m[1] !== 'SubMechanic') seen.get(key).add(filter || '');
  }
}
const lines = ['Logic\tType\tShort\tFilter'];
for (const [key, filters] of [...seen].sort((a, b) => (a[0] < b[0] ? -1 : 1))) {
  // the same name declared twice with different shapes cannot be told apart
  if (filters.size === 1 && [...filters][0]) lines.push(key + '\t' + [...filters][0]);
}
fs.writeFileSync(out, lines.join('\n') + '\n');
console.log(lines.length - 1 + ' filters');
