import fs from 'fs';
import {loadJS} from './load-legacy.mjs';
const f=loadJS();
// ---- recording canvas
function rec(){const s=[];let cur=null,pos=null;
 const r=n=>Math.round(n*1e4)/1e4, S=n=>String(r(n));
 const o={
  beginPath(){}, stroke(){}, fill(){},
  moveTo(x,y){cur=[`M${S(x)} ${S(y)}`];s.push(cur);pos=[x,y]},
  lineTo(x,y){if(!cur){cur=[];s.push(cur)}cur.push(`L${S(x)} ${S(y)}`)},
  arc(x,y,rad,a0,a1,ccw){cur.push(`A${S(x)} ${S(y)} ${S(rad)} ${S(a0)} ${S(a1)} ${ccw?1:0}`)},
  bezierCurveTo(a,b,c,d,e,g){cur.push(`C${[a,b,c,d,e,g].map(S).join(' ')}`)},
  quadraticCurveTo(a,b,c,d){cur.push(`Q${[a,b,c,d].map(S).join(' ')}`)},
 };
 o.out=()=>s.map(a=>a.join(''));return o}
const J=f(null,()=>{},()=>{});  // for tables
const {T,U,PF,SX,GR,KW,LC,PL,NL,pv}=J;
function symOf(t,st){const cx=rec();const g=f(cx,()=>cx.__w=1,()=>{});g.sym(Object.assign({t,on:0,st:0,bl:0,cnt:0,ang:0,k:'sine',f:'50'},st));return cx.out()}
// sym is bound to its own cx per f() call
const UNION=(a,b)=>a.filter(x=>b.includes(x));
function layers(t){
 const base=symOf(t,{});
 const mk=(cond,list)=>list.map(d=>({d,when:cond}));
 const diff=(vs)=>{ // vs: [[cond,state]]
   const sets=vs.map(([c,s])=>[c,symOf(t,s)]);
   const common=sets.map(x=>x[1]).reduce(UNION);
   const out=[];const seen=new Set();
   common.forEach(d=>{if(!seen.has(d)){seen.add(d);out.push({d})}});
   // keep original order of first variant for stable output
   sets.forEach(([c,l])=>{l.filter(d=>!common.includes(d)).forEach(d=>out.push({d,when:c}))});
   return out};
 if(['sw','bt','nb','sp','rl'].includes(t)) return diff([[{state:'on',eq:0},{on:0}],[{state:'on',eq:1},{on:1}]]);
 if(t=='fu') return diff([[{state:'bl',eq:0},{bl:0}],[{state:'bl',eq:1},{bl:1}]]);
 if(['dff','jk','srf','tf'].includes(t)){const l=diff([[{state:'st',eq:0},{st:0}],[{state:'st',eq:1},{st:1}]]);return l}
 if(t=='sg'){const ws=['sine','square','triangle','sawtooth'];return diff(ws.map(w=>[w=='sine'?{param:'k',in:['sine']}:{param:'k',in:[w]},{k:w}]))}
 if(t=='ctr'){const b=symOf(t,{cnt:0});const out=b.map(d=>({d}));for(let k=0;k<4;k++){symOf(t,{cnt:1<<k}).filter(d=>!b.includes(d)).forEach(d=>out.push({d,when:{state:'cnt',bit:k}}))}return out}
 if(t=='mo'){const b=base;const spokes=b.filter(d=>/^M[\d.\-]+ [\d.\-]+L/.test(d)&&!d.startsWith('M-40')&&!d.startsWith('M16 0L40'));const rest=b.filter(d=>!spokes.includes(d));return [...rest.map(d=>({d})),...spokes.map(d=>({d,spin:'ang'}))]}
 if(t=='pot'){const b=base.slice(0,1);return [{d:b[0]},{d:'M0 -40L0 -26L{w} -26L{w} -12',wiper:true},{d:'M{w-4} -17L{w} -11L{w+4} -17',wiper:true}]}
 if(t=='lg')return base.map(d=>({d,fill:'gr',fillWhen:[{when:{state:'hi'},color:'#e11'}]}));
 if(t=='os')return [{widget:'scope'}];
 if(t=='tm')return base.map(d=>({d}));
 return base.map(d=>({d}));
}
// ---- groups
const groupOf={};GR.forEach(([g,ks])=>ks.forEach(k=>groupOf[k]=g));
// ---- pins / optional
const optional=(t,i)=>(t=='dff'&&i>1)||(t=='ctr'&&i>0)||(t=='tm'&&i>5)||(t=='tf'&&i>1)||((t=='jk'||t=='srf')&&i>2);
const G='gnd';
const ALWAYS=/^(r|l|dc|ac|sg|d|sd|led|zd|lamp|mo|am|vm|fu|ldr|ntc|bz|sol)$/;
function dcPath(t){
 if(ALWAYS.test(t))return [{join:[[0,1]]}];
 if(t=='sw'||t=='bt')return [{when:{state:'on',eq:1},join:[[0,1]]}];
 if(t=='nb')return [{when:{state:'on',eq:0},join:[[0,1]]}];
 if(t=='pot')return [{join:[[0,2],[2,1]]}];
 if(t=='sp')return [{when:{state:'on',eq:1},join:[[0,2]]},{when:{state:'on',eq:0},join:[[0,1]]}];
 if(t=='tr')return [{join:[[0,1],[2,3]]}];
 if(t=='rl')return [{join:[[0,1]]},{when:{state:'on',eq:1},join:[[2,3]]}];
 if(t=='npn'||t=='pnp')return [{join:[[1,2],[0,2]]}];
 if(t=='nm'||t=='pm')return [{join:[[1,2]]}];
 if(/^(op|and|or|nand|nor|xor)$/.test(t))return [{join:[[2,G]]}];
 if(t=='not')return [{join:[[1,G]]}];
 if(t=='dff'||t=='tf')return [{join:[[2,G],[3,G]]}];
 if(t=='jk'||t=='srf')return [{join:[[3,G],[4,G]]}];
 if(t=='reg')return [{join:[[2,1]]}];
 if(t=='tm')return [{join:[[0,1],[7,0],[6,0],[5,G],[4,1]]}];
 if(t=='br')return [{join:[[0,2],[1,2],[3,0]]}];
 if(t=='ctr')return [{join:[[2,G],[3,G],[4,G],[5,G]]}];
 return [];
}
function shortPath(t){
 if(t=='sw'||t=='bt')return [{when:{state:'on',eq:1},join:[[0,1]]}];
 if(t=='nb')return [{when:{state:'on',eq:0},join:[[0,1]]}];
 if(t=='am')return [{join:[[0,1]]}];
 if(t=='fu')return [{when:{state:'bl',eq:0},join:[[0,1]]}];
 if(t=='r')return [{when:{value:'lt',than:0.5},join:[[0,1]]}];
 if(t=='sp')return [{when:{state:'on',eq:1},join:[[0,2]]},{when:{state:'on',eq:0},join:[[0,1]]}];
 return null}
// ---- limits
const Q=(q,w,e,msg,u)=>({q,warn:w,err:e,msg,unit:u});
const limits={
 r:[{...Q('power',0.75,1,'overheating ','W'),ref:{key:'q',default:0.25}}],
 lamp:[{...Q('power',1.2,1.6,'overpowered ','W'),ref:{key:'p',default:0.5}}],
 c:[{...Q('peakV',0.8,1,'overvoltage ','V'),ref:{key:'s',default:50}}],
 l:[Q('peakI',3,5,'overcurrent ','A')],
 tr:[Q('avgI',2,4,'winding overcurrent ','A'),Q('peakI',20,40,'winding surge ','A')],
 d:[Q('avgI',.7,1,'overcurrent ','A'),Q('peakI',10,30,'surge ','A'),Q('peakV',80,100,'reverse breakdown ','V')],
 sd:[Q('avgI',2,3,'overcurrent ','A'),Q('peakI',20,40,'surge ','A'),Q('peakV',32,40,'reverse breakdown ','V')],
 led:[Q('avgI',.025,.05,'overcurrent ','A'),Q('peakI',.15,.3,'surge ','A'),Q('peakV',4,6,'reverse voltage ','V')],
 zd:[Q('power',.4,.5,'overpowered ','W')],
 npn:[Q('peakV',32,40,'Vce ','V'),Q('avgI',.15,.4,'collector current ','A'),Q('peakI',.6,1.5,'surge ','A'),Q('power',.4,.6,'dissipating ','W')],
 nm:[Q('peakV',48,60,'Vds ','V'),Q('avgI',1.5,3,'drain current ','A'),Q('peakI',6,12,'surge ','A'),Q('power',.8,1.2,'dissipating ','W')],
 am:[Q('avgI',3,10,'overload ','A')],
};limits.pnp=limits.npn;limits.pm=limits.nm;
// ---- state read by symbol (SF names)
const state={sw:['on'],bt:['on'],nb:['on'],sp:['on'],rl:['on'],fu:['bl'],dff:['st'],jk:['st'],srf:['st'],tf:['st'],ctr:['cnt'],mo:['ang','w'],lg:['av'],led:['gl'],lamp:['gl'],os:['ta','tb'],vm:['av','sq'],am:['av','sq'],om:['av'],bz:['bv']};
const live={pot:{key:'f',min:0,max:100,poke:'pp',scale:0.01},ldr:{key:'f',min:0,max:100,poke:'rv',derived:'ldr'},sol:{key:'f',min:0,max:100,poke:'pp',scale:0.01},ntc:{key:'f',min:-20,max:150,poke:'rv',derived:'ntc'}};
const clickToggle=['sw','bt','nb','sp'];
const bboxOverride={tm:[-54,-74,54,74],os:[-54,-64,174,64]};
const meterLetter={vm:'V',am:'A',om:'Ω',mo:'M'};
const tmText=[['RST',-15,-40,8],['TRG',-15,-20,8],['THR',-15,20,8],['CTL',-15,40,8],['OUT',15,-20,8],['DIS',15,20,8],['VCC',0,-46,8],['GND',0,47,8],['555',0,0,13]];
const suffix={ac:[{key:'f',unit:'Hz'}],sg:[{key:'f',unit:'Hz'}],pot:[{key:'f',unit:'%'}],ldr:[{key:'f',unit:'%'}],sol:[{key:'f',unit:'%'}],ntc:[{key:'f',unit:'°C'}]};
const fmtOf=t=>({vm:'meter',am:'meter',om:'ohm',mo:'motor',lg:'logic',gnd:'none'}[t]||'value');
const prefixes={...PF};
const parts={};
for(const t of Object.keys(T)){
 const e=T[t];
 const params=(SX[t]||[]).map(([key,name,unit,def,opts])=>{
   const o={key,name,unit:unit||''};
   let d=def; if(d===undefined&&(key=='v'||key=='f')&&e[key]!==undefined)d=e[key];
   // T provides initial v/f for parts whose menu entry has no default; SX default wins
   if(d!==undefined)o.default=d; if(opts)o.options=opts;return o});
 // parts whose T has v/f but no params entry
 const P={
  name:e.n, prefix:prefixes[t]??null, group:groupOf[t], keywords:KW[t]||'',
  pins:e.p.map((p,i)=>{const o={x:p[0],y:p[1]};if(PL[t])o.label=PL[t][i];if(optional(t,i))o.optional=true;return o}),
  bbox:bboxOverride[t]?bboxOverride[t]:'auto',
  drawExtent:{ldr:26,ntc:26,sol:24}[t]?{circle:{r:{ldr:26,ntc:26,sol:24}[t]}}:null,
  nonlinear:NL.includes(t),
  rowSource:['dc','ac','sg'].includes(t),
  tolerance:['r','l','c'].includes(t),
  defaults:{v:e.v??null,f:e.f??null},
  params, labelUnit:U[t]??'',
  live:live[t]??null, toggle:clickToggle.includes(t)?(t=='bt'||t=='nb'?'momentary':'latch'):null,
  state:state[t]||[],
  dcPath:dcPath(t), shortPath:shortPath(t), limits:limits[t]||[],
  symbol:layers(t),
 };
 if(t=='led'){P.glow={r:18,color:{param:'p',map:'ledColors'}}}
 if(t=='lamp'){P.glow={r:16,color:'255,210,60'}}
 if(t=='lg'){/* fill described in layers */}
 if(meterLetter[t])P.centerText={text:meterLetter[t],font:'bold 14px',dy:5};
 if(t=='tm')P.pinText=tmText.map(([text,x,y,size])=>({text,x,y,size,bold:size>9}));
 P.valueFormat=fmtOf(t); if(suffix[t])P.valueSuffix=suffix[t];
 if(t=='tm')delete P.drawExtent;
 // prune nulls
 for(const k of Object.keys(P))if(P[k]===null||(Array.isArray(P[k])&&!P[k].length&&!['pins','params','symbol'].includes(k)))delete P[k];
 parts[t]=P;
}
const ledColors=Object.fromEntries(Object.entries(LC).map(([k,v])=>[k,{is:v[0],rgb:v[1]}]));
const out={
 $comment:'CosLog single source of truth. Generated from index.html (JS) and cross-checked against coslogx (Rust). Edit this file, not the editions.',
 schema:1,
 grid:20,
 transform:'mirror X first (x -> -x), then rotate r*90 degrees clockwise; text is never mirrored',
 pathSyntax:'Space-separated numbers per command. M x y | L x y | A cx cy r a0 a1 ccw(0|1) (radians, canvas arc semantics, implicit line to arc start) | C x1 y1 x2 y2 x y | Q x1 y1 x y. Numbers inside {} are wiper expressions: w, w-4, w+4.',
 whenSyntax:{state:'runtime state field name with eq|bit',param:'saved param key with in[]',value:'lt|gt than N on parsed v',hi:'logic probe: av > threshold'},
 stateFields:['vp','ip','is','av','sq','gl','cnt','pc','st','qs','bv','ht','bl','w','ang','cur','pi','pv','ia','pw','on','ix'],
 quantities:{peakI:'pi',avgI:'ia',peakV:'pv',power:'pw'},
 tolerance:{parts:['r','l','c'],applyWhen:'param p > 0 (percent)',formula:'value *= 1 + p/100 * rnd(seed, i)',rnd:'splitmix64: z = seed + (i+1)*0x9E3779B97F4A7C15 (wrapping); z = (z ^ z>>30) * 0xBF58476D1CE4E5B9; z = (z ^ z>>27) * 0x94D049BB133111EB; z ^= z>>31; return (z>>11)/2^52 - 1',defaultSeed:24301,defaultSeedHex:'0x5EED'},
 diagnostics:{
  sourceShortCurrentA:15, sourceRunawayCurrentA:20, ledWarnCurrentA:0.05, nodeVoltageErrorV:1000, lagWarnFrames:3, holdMs:3000, dangleListMax:6, noDcPathListMax:6,
  messages:{
   empty:'Place parts, wire them (click a pin to start a wire), add a Ground, then press Run.',
   noGround:'No Ground part: node voltages and meters have no reference. Add a Ground.',
   shortedTerminals:'{name} has both terminals on one node (shorted by wiring).',
   dangling:'Unconnected pin(s): {list}{more}. Wire them or remove the part.',
   noDcPath:'No DC path to ground at: {list}. Readings there are unreliable. Connect to ground or add a bleed resistor (capacitors and open switches block DC).',
   info:'{parts} parts, {nodes} nodes, step {dt}{newton}{farm}',
   newton:', Newton solver active',
   farm:', {n} independent section{s} on {w} thread{s2}',
   noConverge:'Solver did not converge in {n} step(s) (last change {md}). Try: series resistance on diodes/LEDs/sources, lower op-amp gain, check transistor polarity, or lower the speed.',
   lag:'Cannot keep real-time on this CPU, so the simulation runs slower than the speed setting. {hint}',
   lagHintAuto:'Auto quality has coarsened the time step to keep up.',
   lagHintManual:'Set Quality to Auto or Fast, or simplify the circuit.',
   nodeVoltage:'Node voltage reached {v}: usually a floating node, a shorted ideal source, or a missing ground path.',
   sourceRunaway:'{name} supplies {i}: probably a short circuit. Add series resistance.',
   ledCurrent:'{name} carries {i}: add a current-limiting resistor.',
   fuseBlown:'{name} has blown (open). Press Reset to replace it.',
   shortTrace:'{name} is short-circuited through {via}. Marked in red on the schematic.',
   shortWiring:'wiring',
   partFault:'{name}: {msg}.',
   limitErr:'{msg}{value} exceeds {limit}', limitWarn:'{msg}{value} near limit {limit}',
   sourceShort:'short circuit: supplying {i}', fuseFault:'blown (open)', pinFault:'pin {n} unconnected', faultShort:'short circuit', faultPath:'in short-circuit path'
  },
  nameRule:'display name = n if non-empty, else part code'
 },
 formatters:{value:'v + (u ?? labelUnit) + valueSuffix entries (" {f}{unit}"); empty when v is absent',meter:'rms if ac > 2% of rms and > 1e-6, else average; units V or A',ohm:'OL if av > 1e8 else fm(av, "Ω")',motor:'round(w * 9.549) + " rpm"',logic:'HIGH if av > threshold else LOW',none:'(blank)'},
 ledColors, groups:GR.map(([name,ids])=>({name,parts:ids})), parts
};

fs.writeFileSync(new URL('../parts.json',import.meta.url).pathname,JSON.stringify(out,null,1));
console.log(Object.keys(parts).length, fs.statSync(new URL('../parts.json',import.meta.url).pathname).size);
