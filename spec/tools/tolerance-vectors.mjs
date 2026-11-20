const M=(1n<<64n)-1n;
export function rnd(seed,i){let z=(BigInt(seed)+BigInt(i+1)*0x9E3779B97F4A7C15n)&M;z=((z^(z>>30n))*0xBF58476D1CE4E5B9n)&M;z=((z^(z>>27n))*0x94D049BB133111EBn)&M;z^=z>>31n;return Number(z>>11n)/2**52-1}
for(const [s,i] of [[0x5EED,0],[0x5EED,1],[0x5EED,2],[0x5EED,7],[0x5EED,100],[1,0],[1,3]])console.log(s,i,rnd(s,i).toPrecision(17))
