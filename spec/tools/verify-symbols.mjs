import fs from 'fs';
import {loadJS} from './load-legacy.mjs';
const f=loadJS();
const spec=JSON.parse(fs.readFileSync(new URL('../parts.json',import.meta.url).pathname,'utf8'));
function rec(){const s=[];let cur;const S=n=>String(Math.round(n*1e4)/1e4);return{beginPath(){},stroke(){},fill(){},
 moveTo(x,y){cur=[`M${S(x)} ${S(y)}`];s.push(cur)},lineTo(x,y){if(!cur){cur=[];s.push(cur)}cur.push(`L${S(x)} ${S(y)}`)},
 arc(x,y,r,a,b,c){cur.push(`A${S(x)} ${S(y)} ${S(r)} ${S(a)} ${S(b)} ${c?1:0}`)},bezierCurveTo(...a){cur.push('C'+a.map(S).join(' '))},quadraticCurveTo(...a){cur.push('Q'+a.map(S).join(' '))},out:()=>s.map(a=>a.join(''))}}
const ok=(cond,c)=>{if(!cond)return true;if(cond.state){const v=c[cond.state];if(cond.bit!==undefined)return ((v|0)>>cond.bit)&1;return v==cond.eq}if(cond.param)return cond.in.includes(c[cond.param]||'sine');return true};
const render=(t,c)=>{const p=spec.parts[t];const w=-30+60*Math.min(1,Math.max(0,(+c.f||0)/100));return p.symbol.filter(l=>l.d&&ok(l.when,c)).map(l=>l.wiper?l.d.replace(/\{w([+-]\d+)?\}/g,(m,o)=>w+(o?+o:0)):l.d)};
let bad=0,n=0;
const states=[];for(const on of [0,1])for(const st of [0,1])for(const bl of [0,1])for(const k of ['sine','square','triangle','sawtooth'])for(const cnt of [0,1,2,4,8,15])for(const fv of ['0','50','100'])states.push({on,st,bl,k,cnt,f:fv,ang:0});
for(const t of Object.keys(spec.parts)){ if(t=='os')continue;
 for(const s of states){const cx=rec();const g=f(cx,()=>{},()=>{});const c={t,...s};g.sym(c);const a=cx.out().slice().sort().join('|');const b=render(t,c).slice().sort().join('|');n++;
  if(a!=b){bad++;if(bad<6)console.log('MISMATCH',t,JSON.stringify(s),'\n',a,'\n',b)}}}
console.log('checked',n,'bad',bad);
