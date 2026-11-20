// One-time migration of the legacy single-file app (index.html at commit 75f6205) into
// coslog-y/src/template.html. The data tables and sym() are replaced by a runtime that reads
// the compact spec injected by `cargo xtask build-y`. Usage:
//   git show 75f6205:index.html > /tmp/legacy.html
//   node coslog-y/tools/migrate-legacy.mjs /tmp/legacy.html coslog-y/src/template.html
import fs from 'fs';
const [,, src, dst] = process.argv;
const L = fs.readFileSync(src, 'utf8').split('\n');
const at = (n) => L[n - 1];
const must = (n, re) => { if (!re.test(at(n))) throw new Error(`line ${n} does not match ${re}: ${at(n).slice(0, 80)}`); };

// ---- sanity: the line ranges we replace
must(27, /^const T=\{dc:/); must(56, /^const KW=/); must(54, /T\.os\.b=\[-54,-64,174,64\];let lt=0;/);
must(229, /^function Pp\(/); must(230, /^function sym\(c\)\{/); must(271, /^ cx\.stroke\(\)\}$/); must(272, /^const PL=/);
for (const n of [389, 390, 391, 437, 469, 470]) must(n, /^(Object\.assign\((U|PF)|SX\.|SX\.r\.push)/);

const RUNTIME = `/* ---- part tables, derived from the compact spec injected at build time (spec/parts.json) ---- */
const SPEC=/*@@SPEC@@*/null;
const T={},U={},PF={},SX={},KW={},LC={},PL={},GR=SPEC.G,NL=[];
for(const k in SPEC.P){const s=SPEC.P[k];T[k]={n:s.n,p:s.p,v:s.v,f:s.f};if(s.x)PF[k]=s.x;if(s.u)U[k]=s.u;if(s.a)SX[k]=s.a;if(s.l)PL[k]=s.l;if(s.N)NL.push(k);KW[k]=s.k;
 const p=s.p,t=T[k];t.b=s.b||[Math.min(...p.map(a=>a[0]))-14,Math.min(...p.map(a=>a[1]))-14,Math.max(...p.map(a=>a[0]))+14,Math.max(...p.map(a=>a[1]))+14]}
for(const k in SPEC.L)LC[k]=SPEC.L[k];
let lt=0;`;

const SYM = `const PC={};
function pcmds(d){return PC[d]||(PC[d]=d.match(/[MLACQ][^MLACQ]*/g).map(s=>[s[0],...s.slice(1).trim().split(' ').map(Number)]))}
function pwip(d,w){return pcmds(d.replace(/\\{w([+-]\\d+)?\\}/g,(m,o)=>w+(o?+o:0)))}
function lok(c,w){const t=w[0],k=w[1];if(t=='s')return(+c[k]||0)==w[2];if(t=='b')return((c[k]|0)>>w[2])&1;const o=(SX[c.t].find(e=>e[0]==k)||[])[4],v=o&&!w[2].includes(c[k])&&!o.includes(c[k])?o[0]:c[k];return w[2].includes(v)}
function sym(c){const t=c.t;cx.beginPath();
 for(const L of SPEC.P[t].y){let d=L,w=null,fl=0;if(typeof L!='string'){if(L[0]=='@scope'){osc(c);continue}d=L[0];w=L[1];fl=L[2]|0}
  if(w&&!lok(c,w))continue;
  let cm;if(fl==1){const x=-30+60*Math.min(1,Math.max(0,(pv(c.f)||0)/100));cm=pwip(d,x)}else cm=pcmds(d);
  if(fl==2){cx.save();cx.rotate(c.ang)}
  for(const m of cm)m[0]=='M'?cx.moveTo(m[1],m[2]):m[0]=='L'?cx.lineTo(m[1],m[2]):m[0]=='A'?cx.arc(m[1],m[2],m[3],m[4],m[5],m[6]):m[0]=='C'?cx.bezierCurveTo(m[1],m[2],m[3],m[4],m[5],m[6]):cx.quadraticCurveTo(m[1],m[2],m[3],m[4]);
  if(fl==2)cx.restore()}
 cx.stroke();if(t=='tm')tml(c)}`;

// ---- application-level edits (anchor strings are asserted)
const edit = (s, a, b) => { if (s.split(a).length !== 2) throw new Error('anchor not unique/found: ' + a.slice(0, 60)); return s.replace(a, () => b); };
const drop = new Set([389, 390, 391, 437, 469, 470, 272]);
const out = [];
for (let n = 1; n <= L.length; n++) {
  if (n === 27) { out.push(RUNTIME); continue; }
  if (n > 27 && n <= 56) continue;
  if (n === 230) { out.push(SYM); continue; }
  if (n > 230 && n <= 271) continue;
  if (drop.has(n)) continue;
  out.push(L[n - 1]);
}
let s = out.join('\n');

// tolerance: deterministic splitmix64 keyed by seed and file index (spec/project_schema.md section 4)
s = edit(s, 'comps.forEach(c=>{c.nd=c.id.map', 'comps.forEach((c,ci)=>{c.nd=c.id.map');
s = edit(s, 'if(c.rnd===undefined)c.rnd=Math.random()*2-1;', 'c.rnd=rnd64(SEED,c.si!==undefined?c.si:ci);');
s = edit(s, 'function build(){', `let SEED=24301;
const M64=(1n<<64n)-1n;
function rnd64(seed,i){let z=(BigInt(seed)+BigInt(i+1)*0x9E3779B97F4A7C15n)&M64;z=((z^(z>>30n))*0xBF58476D1CE4E5B9n)&M64;z=((z^(z>>27n))*0x94D049BB133111EBn)&M64;z^=z>>31n;return Number(z>>11n)/2**52-1}
function build(){`);

// load: whitelist the 16 saved keys, coerce types, skip unknown part codes with a warning
s = edit(s, 'function load(s){const o=JSON.parse(s);comps=o.c.map(a=>Object.assign(mk(a.t,a.x,a.y,a.r),a));wires=o.w;selUI();mark()}',
`const SK=['v','f','p','q','s','k','n','u'];
let skipped=[];
function load(s){const o=JSON.parse(s);skipped=[];comps=[];o.c.forEach((a,i)=>{if(!a||!T[a.t]){skipped.push(a&&a.t);return}
 const c=mk(a.t,Math.round(+a.x||0),Math.round(+a.y||0),((+a.r|0)%4+4)%4);c.si=i;c.m=a.m&&a.m!=='0'?1:0;c.on=a.on&&a.on!=='0'?1:0;
 for(const k of SK)if(a[k]!==undefined&&a[k]!==null&&!(k=='n'&&a[k]===''))c[k]=String(a[k]);
 for(const k of['no','vo'])if(Array.isArray(a[k])&&a[k].length==2)c[k]=[+a[k][0]||0,+a[k][1]||0];
 comps.push(c)});
 wires=o.w.map(w=>({x1:Math.round(w.x1),y1:Math.round(w.y1),x2:Math.round(w.x2),y2:Math.round(w.y2)}));selUI();mark()}`);
s = edit(s, "function projTxt(){return JSON.stringify(Object.assign({app:'CircuitLite',ver:1},JSON.parse(snap()),{view}),null,1)}",
 "function projTxt(){return JSON.stringify(Object.assign({app:'CosLog',ver:1},SEED!=24301?{seed:SEED}:{},JSON.parse(snap()),{view}),null,1)}");
s = edit(s, 'function loadProj(t){const o=JSON.parse(t);if(!Array.isArray(o.c)||!Array.isArray(o.w))throw 0;',
 'function loadProj(t){const o=JSON.parse(t);if(!Array.isArray(o.c)||!Array.isArray(o.w))throw 0;{const sd=typeof o.seed==\'string\'?Number(o.seed):o.seed;SEED=Number.isSafeInteger(sd)&&sd>=0?sd:24301}');
s = edit(s, 'push();load(JSON.stringify(o));', "push();load(JSON.stringify(o));if(skipped.length)flash('Skipped '+skipped.length+' unknown part(s): '+[...new Set(skipped)].join(', ')+' (made with a newer or different CosLog)');");
s = edit(s, "document.title=(u?'* ':'')+'CircuitLite'", "document.title=(u?'* ':'')+'CosLog Y'");
s = edit(s, "+'  |  step '+(net?fm(net.dt,'s'):'-')", "+'  |  step '+(net?fm(net.dt,'s'):'-')+'  |  spec '+SPEC.h.slice(0,8)");
s = s.replace('<title>CircuitLite</title>', '<title>CosLog Y</title>');
fs.writeFileSync(dst, s);
console.log('wrote', dst, s.length, 'bytes');
