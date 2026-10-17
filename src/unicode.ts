export const unicodeCode=(n:number)=>`U+${n.toString(16).toUpperCase().padStart(4,'0')}`;
export function glyphLabel(character:string){
  if(character===' ')return '␣';
  if(!character||/[\p{C}\p{Z}]/u.test(character))return '◌';
  return /\p{M}/u.test(character)?`◌${character}`:character;
}
export function matchesGlyph(g:{character:string;name:string;codepoints:number[];cid?:number|null},query:string){
  const q=query.trim().toUpperCase();
  return !q||(g.cid!=null&&`CID${g.cid}`.includes(q.replace(/\s/g,'')))||g.character.includes(query)||g.name.toUpperCase().includes(q)||g.codepoints.some(n=>unicodeCode(n).includes(q)||n.toString(16).toUpperCase().padStart(4,'0')===q);
}
