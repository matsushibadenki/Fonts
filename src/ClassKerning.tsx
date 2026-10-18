import {useEffect,useMemo,useState} from 'react';
import {IconButton} from './Panel';
import {Icon} from './icons';
import {proofCommand} from './ProofControls';
import {kerningLabels} from './kerning-i18n';
import {classKerningLabels} from './class-kerning-i18n';
import {referenceError} from './reference-i18n';
import type {Edit,Snapshot} from './types';
export function ClassKerning({state,edit}:{state:Snapshot;edit:(a:Edit)=>Promise<void>}){
 const t=classKerningLabels[state.workspace.locale];const k=kerningLabels[state.workspace.locale];const library=state.class_kerning;
 const [side,setSide]=useState<'left'|'right'>('left');const [id,setId]=useState('');const [name,setName]=useState('');const [members,setMembers]=useState('');const [left,setLeft]=useState('');const [right,setRight]=useState('');const [value,setValue]=useState('0');const [error,setError]=useState('');const [busy,setBusy]=useState(false);
 const glyphs=useMemo(()=>new Map(state.glyphs.map(g=>[g.id,g])),[state.glyphs]);
 const names=useMemo(()=>{const m=new Map<string,number>();for(const g of state.glyphs){if(g.id===0)continue;m.set(g.name,g.id);m.set(`#${g.id}`,g.id);for(const c of g.codepoints)m.set(String.fromCodePoint(c),g.id);}return m;},[state.glyphs]);
 const current=library[side].find(c=>String(c.id)===id);const memberKey=current?.glyphs.join(' ')??'';
 useEffect(()=>{setName(current?.name??'');setMembers(current?current.glyphs.map(id=>`#${id}`).join(' '):(state.glyph?`#${state.glyph}`:''));},[side,id,current?.name,memberKey,state.glyph]);
 const lc=library.left.find(c=>String(c.id)===left)??library.left[0];const rc=library.right.find(c=>String(c.id)===right)??library.right[0];
 const stored=library.pairs.find(p=>p.left===lc?.id&&p.right===rc?.id)?.value??0;
 useEffect(()=>setValue(String(stored)),[lc?.id,rc?.id,stored]);
 const command=async(operation:Record<string,unknown>)=>{setBusy(true);try{await edit({type:'class_kerning',operation,revision:state.revision});setError('');return true;}catch(e){setError(String(e)==='TooLarge'?t.limit:String(e)==='InvalidDocument'||String(e)==='MissingGlyph'?t.invalid:referenceError(state.workspace.locale,e));return false;}finally{setBusy(false);}};
 const save=async()=>{const ids=new Set<number>();for(const token of members.split(/[\s,]+/u).filter(Boolean)){if(token.startsWith('#')){const match=/^#([0-9]+)$/.exec(token);const id=match?Number(match[1]):0;if(!Number.isSafeInteger(id)||id===0||!glyphs.has(id)){setError(t.invalid);return;}ids.add(id);continue;}const exact=names.get(token);if(exact!==undefined){ids.add(exact);continue;}for(const c of Array.from(token)){const g=names.get(c);if(g===undefined){setError(t.invalid);return;}ids.add(g);}}if(!name.trim()||!ids.size){setError(t.invalid);return;}const next=current?.id??library.next_id;if(await command({type:'upsert',side,id:current?.id??null,name:name.trim(),glyphs:[...ids].sort((a,b)=>a-b)}))setId(String(next));};
 const number=Number(value);const valid=!!lc&&!!rc&&value.trim()!==''&&Number.isInteger(number)&&number>=-32768&&number<=32767;
 const preview=()=>{if(!lc||!rc)return;const l=lc.glyphs.map(id=>glyphs.get(id)?.codepoints[0]).filter((c):c is number=>c!==undefined).slice(0,4);const r=rc.glyphs.map(id=>glyphs.get(id)?.codepoints[0]).filter((c):c is number=>c!==undefined).slice(0,4);if(!l.length||!r.length){setError(k.missing);return;}const text=l.flatMap(a=>r.map(b=>String.fromCodePoint(a,b))).join(' ');void proofCommand(state,{type:'prepare',settings:{...state.proof.settings,text,kerning:true,scroll:0}}).catch(e=>setError(referenceError(state.workspace.locale,e)));};
 return <details className="class-kerning" open><summary>{t.title}<span className="class-count">{library.left.length} / {library.right.length}</span></summary>
  <div className="class-toolbar"><button aria-pressed={side==='left'} onClick={()=>{setSide('left');setId('');}}>{t.left}</button><button aria-pressed={side==='right'} onClick={()=>{setSide('right');setId('');}}>{t.right}</button><span className="tool-spacer"/><span className="panel-help" role="img" tabIndex={0} title={t.help} aria-label={t.help}><Icon name="info"/></span></div>
  <select aria-label={t.choose} value={current?String(current.id):''} onChange={e=>setId(e.target.value)}><option value="">＋ {t.newClass}</option>{library[side].map(c=><option key={c.id} value={c.id}>{c.name} ({c.glyphs.length})</option>)}</select>
  <input aria-label={t.name} placeholder={t.name} maxLength={80} value={name} onChange={e=>setName(e.target.value)}/>
  <textarea aria-label={t.members} placeholder={t.members} rows={2} maxLength={32768} value={members} onChange={e=>setMembers(e.target.value)}/>
  <div className="class-toolbar"><IconButton icon="check" label={t.save} disabled={busy||!name.trim()||!members.trim()} onClick={()=>void save()}/><IconButton icon="trash" label={t.remove} disabled={busy||!current} onClick={()=>{if(current)void command({type:'remove',side,id:current.id}).then(ok=>{if(ok)setId('');});}}/><span className="tool-spacer"/><span className="class-count">{library.pairs.length}</span></div>
  <div className="class-pair-select"><select aria-label={t.leftPair} value={lc?.id??''} onChange={e=>setLeft(e.target.value)}>{!library.left.length&&<option value="">—</option>}{library.left.map(c=><option key={c.id} value={c.id}>{c.name}</option>)}</select><span>↔</span><select aria-label={t.rightPair} value={rc?.id??''} onChange={e=>setRight(e.target.value)}>{!library.right.length&&<option value="">—</option>}{library.right.map(c=><option key={c.id} value={c.id}>{c.name}</option>)}</select></div>
  <div className="kerning-actions"><input aria-label={t.title+' · '+k.value} title={t.pairHelp} type="number" min={-32768} max={32767} step={1} value={value} onChange={e=>setValue(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&valid&&!busy)void command({type:'pair',left:lc!.id,right:rc!.id,value:number});}}/><IconButton icon="check" label={t.apply} disabled={!valid||busy} onClick={()=>void command({type:'pair',left:lc!.id,right:rc!.id,value:number})}/><IconButton icon="eye" label={t.preview} disabled={!lc||!rc||busy} onClick={preview}/></div>
  <div className="kerning-pairs">{library.pairs.filter(p=>p.left===lc?.id).map(p=>{const r=library.right.find(c=>c.id===p.right);return <div className="kerning-row" key={p.right}><button title={`${lc?.name} ↔ ${r?.name}`} onClick={()=>{setRight(String(p.right));setValue(String(p.value));}}><span>{r?.name}</span><span>{p.value>0?'+':''}{p.value}</span></button><IconButton icon="trash" label={`${t.removePair} ${r?.name}`} disabled={busy} onClick={()=>void command({type:'pair',left:p.left,right:p.right,value:0})}/></div>;})}</div>
  {error&&<p className="field-error" role="alert">{error}</p>}
 </details>;
}
