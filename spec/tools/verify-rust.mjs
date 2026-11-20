import fs from 'fs';
const spec=JSON.parse(fs.readFileSync(new URL('../parts.json',import.meta.url).pathname,'utf8')).parts;
const src=fs.readFileSync(new URL('../../coslogx/circuit_core/src/kind.rs',import.meta.url).pathname,'utf8');
const nm=src.slice(src.indexOf('pub fn name'),src.indexOf('pub fn pins'));
const code=src.match(/Dc="dc".*?\n\}/s)[0];
const kinds=Object.fromEntries([...code.matchAll(/(\w+)="(\w+)"/g)].map(m=>[m[1],m[2]]));
let bad=0;
for(const m of nm.matchAll(/(\w+)=>"([^"]+)"/g)){const t=kinds[m[1]];if(!t)continue;if(spec[t].name!==m[2]){bad++;console.log('name',t,spec[t].name,m[2])}}
const P2=[[-40,0],[40,0]],SRC=[[0,-40],[0,40]],BJT=[[-40,0],[20,-40],[20,40]],GATE=[[-40,-20],[-40,20],[40,0]],Q4=[[-40,-20],[-40,20],[40,-20],[40,20]],FF3=[[-40,-20],[-40,0],[-40,20],[40,-20],[40,20]];
const rp=k=>{if('dc ac cs sol sg'.includes(k)&&['dc','ac','cs','sol','sg'].includes(k))return SRC;if(['gnd','lg'].includes(k))return [[0,0]];if(['tr','rl','dff','br','tf'].includes(k))return Q4;if(['npn','pnp','nm','pm'].includes(k))return BJT;if(k=='pot')return [[-40,0],[40,0],[0,-40]];if(k=='not')return P2;if(['op','and','or','nand','nor','xor'].includes(k))return GATE;if(k=='os')return [[-40,-20],[-40,20]];if(k=='sp')return [[-40,0],[40,-20],[40,20]];if(k=='reg')return [[-40,-20],[0,40],[40,-20]];if(k=='ctr')return [[-40,-20],[-40,20],[40,-40],[40,-20],[40,20],[40,40]];if(k=='tm')return [[0,-60],[0,60],[-40,-20],[-40,20],[40,20],[40,-20],[-40,-40],[-40,40]];if(['jk','srf'].includes(k))return FF3;return P2};
for(const t of Object.keys(spec)){const a=JSON.stringify(spec[t].pins.map(p=>[p.x,p.y])),b=JSON.stringify(rp(t));if(a!==b){bad++;console.log('pins',t,a,b)}}
console.log('bad',bad)
