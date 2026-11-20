// Stage 2 of the Y template build-up (run after migrate-legacy.mjs): replaces the hard-coded diagnostics
// (limits, DC paths, short paths, optional pins, messages, thresholds) with lookups into SPEC.
// Usage: node coslog-y/tools/wire-spec.mjs coslog-y/src/template.html
import fs from 'node:fs';
const f = process.argv[2];
let s = fs.readFileSync(f, 'utf8');
const edit = (a, b) => { if (!s.includes(a)) { console.error('missing anchor:\n' + a.slice(0, 160)); process.exit(1); } s = s.replace(a, () => b); };
const cut = (from, to, b) => { const i = s.indexOf(from), j = s.indexOf(to, i); if (i < 0 || j < 0) { console.error('missing range ' + from.slice(0, 60)); process.exit(1); } s = s.slice(0, i) + b + s.slice(j); };

// helpers: message templates, `when` evaluation for path rules, rule runner, optional pins
edit("function lok(c,w){const t=w[0],k=w[1];", String.raw`const M=(k,o)=>SPEC.M[k].replace(/\{(\w+)\}/g,(_,x)=>o?o[x]:''),K=SPEC.K;
function jr(c,key,fn){let n=0;for(const r of SPEC.P[c.t][key]||[])if(!r[0]||lok(c,r[0]))for(const[a,b]of r[1]){fn(a<0?0:c.nd[a],b<0?0:c.nd[b]);n++}return n}
const opt=(c,i)=>(SPEC.P[c.t].o||[]).includes(i);
function lok(c,w){const t=w[0],k=w[1];if(t=='v')return k=='lt'?c.val<w[2]:c.val>w[2];`);

// statics
edit("return[['i','Place parts, wire them (click a pin to start a wire), add a Ground, then press Run.']];", "return[['i',M('empty')]];");
edit("o.push(['e','No Ground part: node voltages and meters have no reference. Add a Ground.'])", "o.push(['e',M('noGround')])");
edit("o.push(['e',nm(c)+' has both terminals on one node (shorted by wiring).'])", "o.push(['e',M('shortedTerminals',{name:nm(c)})])");
cut("  if(/^(r|l|dc|ac|sg|d|sd|led|zd|lamp|mo|am|vm|fu|ldr|ntc|bz|sol)$/.test(t)", "const dang=[],fl=[]", "  jr(c,'d',u)});\n ");
edit("if(k&&deg[k]==1&&!((c.t=='dff'&&i>1)||(c.t=='ctr'&&i>0)||(c.t=='tm'&&i>5)||(c.t=='tf'&&i>1)||((c.t=='jk'||c.t=='srf')&&i>2)))dang.push", "if(k&&deg[k]==1&&!opt(c,i))dang.push");
edit("o.push(['w','Unconnected pin(s): '+dang.slice(0,6).join(', ')+(dang.length>6?'...':'')+'. Wire them or remove the part.'])", "o.push(['w',M('dangling',{list:dang.slice(0,K.dm).join(', '),more:dang.length>K.dm?'...':''})])");
edit("o.push(['w','No DC path to ground at: '+fl.slice(0,6).join(', ')+'. Readings there are unreliable. Connect to ground or add a bleed resistor (capacitors and open switches block DC).'])", "o.push(['w',M('noDcPath',{list:fl.slice(0,K.nm).join(', ')})])");
edit("o.push(['i',comps.length+' parts, '+(N-1)+' nodes, step '+fm(net.dt,'s')+(net.q?', Newton solver active':'')+farmInfo()])", "o.push(['i',M('info',{parts:comps.length,nodes:N-1,dt:fm(net.dt,'s'),newton:net.q?M('newton'):'',farm:farmInfo()})])");
edit("function farmInfo(){const n=farm.gs.length,w=nwFor(n);return(n?', '+n+' independent section'+(n>1?'s':'')+' on '+w+' thread'+(w>1?'s':''):'')+(farm.note?'. '+farm.note:'')}", "function farmInfo(){const n=farm.gs.length,w=nwFor(n);return(n?M('farm',{n,s:n>1?'s':'',w,s2:w>1?'s':''}):'')+(farm.note?'. '+farm.note:'')}");

// dyn
edit("o.push(['e','Solver did not converge in '+net.fail+' step(s) (last change '+fm(net.md,'V')+'). Try: series resistance on diodes/LEDs/sources, lower op-amp gain, check transistor polarity, or lower the speed.'])", "o.push(['e',M('noConverge',{n:net.fail,md:fm(net.md,'V')})])");
edit("if(net.lag>3)o.push(['w','Cannot keep real-time on this CPU, so the simulation runs slower than the speed setting. '+(qa?(qs>1?'Auto quality has coarsened the time step to keep up.':''):'Set Quality to Auto or Fast, or simplify the circuit.')])", "if(net.lag>K.lf)o.push(['w',M('lag',{hint:qa?(qs>1?M('lagHintAuto'):''):M('lagHintManual')})])");
edit("if(net.mv>1e3)o.push(['e','Node voltage reached '+fm(net.mv,'V')+': usually a floating node, a shorted ideal source, or a missing ground path.'])", "if(net.mv>K.nv)o.push(['e',M('nodeVoltage',{v:fm(net.mv,'V')})])");
edit("if(c.row>=0&&Math.abs(c.ix||0)>20)o.push(['e',n+' supplies '+fm(Math.abs(c.ix||0),'A')+': probably a short circuit. Add series resistance.'])", "if(c.row>=0&&Math.abs(c.ix||0)>K.sr)o.push(['e',M('sourceRunaway',{name:n,i:fm(Math.abs(c.ix||0),'A')})])");
edit("if(c.t=='led'&&c.cur>.05)o.push(['w',n+' carries '+fm(c.cur,'A')+': add a current-limiting resistor.'])", "if(c.t=='led'&&c.cur>K.lw)o.push(['w',M('ledCurrent',{name:n,i:fm(c.cur,'A')})])");
edit("if(c.t=='fu'&&c.bl)o.push(['i',n+' has blown (open). Press Reset to replace it.'])", "if(c.t=='fu'&&c.bl)o.push(['i',M('fuseBlown',{name:n})])");

// chk: spec limits
cut("function chk(c){", "function scan(o){", String.raw`function chk(c){const P=SPEC.P[c.t];
 for(const l of P.g||[]){const a=c[SPEC.Q[l[0]]]||0,rt=l[5]?(c[l[5]+'n']>0?c[l[5]+'n']:l[6]):1,w=l[1]*rt,e=l[2]*rt;
  if(a>e)return['e',M('limitErr',{msg:l[3],value:fm(a,l[4]),limit:fm(e,l[4])})];if(a>w)return['w',M('limitWarn',{msg:l[3],value:fm(a,l[4]),limit:fm(e,l[4])})]}
 if(c.row>=0){const A=c.ia||0;return A>K.sa?['e',M('sourceShort',{i:fm(A,'A')})]:null}
 if(P.B&&c.bl)return['w',M('fuseFault')];return null}
`);

// scan
edit("const now=performance.now(),hold=now+3000,", "const now=performance.now(),hold=now+K.h,");
edit("if(k&&deg[k]==1&&!((c.t=='dff'&&i>1)||(c.t=='ctr'&&i>0)||(c.t=='tm'&&i>5)||(c.t=='tf'&&i>1)||((c.t=='jk'||c.t=='srf')&&i>2))){if(!c.ft||c.ft.l!='e')c.ft={l:'w',m:'pin '+(i+1)+' unconnected'", "if(k&&deg[k]==1&&!opt(c,i)){if(!c.ft||c.ft.l!='e')c.ft={l:'w',m:M('pinFault',{n:i+1})");
cut("comps.forEach(c=>{const n=c.nd,t=c.t;if(((t=='sw'||t=='bt')&&c.on)", "\n comps.forEach(c=>{if(c.row>=0&&f(c.nd[0])", "comps.forEach(c=>{if(jr(c,'s',(a,b)=>{uf[f(a)]=f(b)}))low.push(c)});");
edit("mC(c,'e','short circuit');via.forEach(l=>mC(l,'e','in short-circuit path'))", "mC(c,'e',M('faultShort'));via.forEach(l=>mC(l,'e',M('faultPath')))");
edit("o.push(['e',nm(c)+' is short-circuited through '+(via.length?via.map(nm).join(', '):'wiring')+'. Marked in red on the schematic.'])", "o.push(['e',M('shortTrace',{name:nm(c),via:via.length?via.map(nm).join(', '):M('shortWiring')})])");
edit("o.push([r[0],nm(c)+': '+r[1]+'.'])", "o.push([r[0],M('partFault',{name:nm(c),msg:r[1]})])");
edit("if(net.mv>1e3){const bn=net.bn;", "if(net.mv>K.nv){const bn=net.bn;");

// load warnings: odd app, newer version, unreadable seed, skipped parts (one banner, shown longer than a normal flash)
edit("if(run)setRun(false);push();load(JSON.stringify(o));if(skipped.length)flash('Skipped '+skipped.length+' unknown part(s): '+[...new Set(skipped)].join(', ')+' (made with a newer or different CosLog)');", String.raw`const wn=[];if(o.app!==undefined&&o.app!='CosLog'&&o.app!='CircuitLite')wn.push('Unrecognised app "'+o.app+'"');if(o.ver>1)wn.push('This file was saved by a newer CosLog');if(o.seed!==undefined&&o.seed!==null&&!(Number.isSafeInteger(typeof o.seed=='string'?Number(o.seed):o.seed)&&(typeof o.seed=='string'?Number(o.seed):o.seed)>=0))wn.push('Unreadable tolerance seed; using the default');
if(run)setRun(false);push();load(JSON.stringify(o));if(skipped.length)wn.push('Skipped '+skipped.length+' unknown part(s): '+[...new Set(skipped)].join(', ')+' (made with a newer or different CosLog)');if(wn.length)flash(wn.join('; '),9000);`);
edit("flash=m=>{$('us').textContent=m+'  ';setTimeout(upd,2500)}", "flash=(m,d)=>{$('us').textContent=m+'  ';setTimeout(upd,d||2500)}");
fs.writeFileSync(f, s);
console.log('wired', f, s.length, 'bytes');
