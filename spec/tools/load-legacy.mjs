import fs from 'fs';
const L=fs.readFileSync(process.env.LEGACY_HTML||new URL('../../index.html',import.meta.url).pathname,'utf8').split('\n');
const seg=(a,b)=>L.slice(a-1,b).join('\n');
export function loadJS(){
 const code=`const PI=Math.PI,P2=[[-40,0],[40,0]];
 ${seg(27,56)}
 ${seg(63,64)}
 ${seg(272,272)}
 ${seg(389,391)}
 ${seg(437,437)}
 ${seg(469,470)}
 function Pp(a){cx.moveTo(a[0],a[1]);for(let i=2;i<a.length;i+=2)cx.lineTo(a[i],a[i+1])}
 ${seg(230,271)}
 return {T,U,PF,SX,GR,KW,LC,PL,NL,sym,pv};`;
 return new Function('cx','osc','tml',code);
}
