import {ClassKerning} from './ClassKerning';
import {classKerningLabels} from './class-kerning-i18n';
import {kerningLabels} from './kerning-i18n';
import {useDeferredValue,useEffect,useMemo,useState} from 'react';
import {IconButton} from './Panel';
import {Icon} from './icons';
import {proofCommand} from './ProofControls';
import {referenceError} from './reference-i18n';
import type {Edit,Snapshot} from './types';
export function Kerning({state,edit}:{state:Snapshot;edit:(a:Edit)=>Promise<void>}){
 const t=kerningLabels[state.workspace.locale];const [query,setQuery]=useState('');const search=useDeferredValue(query);const [value,setValue]=useState('0');const [error,setError]=useState('');const [busy,setBusy]=useState(false);
 const glyphs=useMemo(()=>new Map(state.glyphs.map(g=>[g.id,g])),[state.glyphs]);
 const paired=useMemo(()=>new Set(state.kerning.map(p=>p.right)),[state.kerning]);
 const target=state.glyphs.find(g=>g.id!==0&&(query===`#${g.id}`||query===g.name||g.codepoints.some(c=>String.fromCodePoint(c)===query)));
 const ct=classKerningLabels[state.workspace.locale];
 const classLeft=state.class_kerning.left.find(c=>c.glyphs.includes(state.glyph));const classRight=state.class_kerning.right.find(c=>c.glyphs.includes(target?.id??0));const inherited=state.class_kerning.pairs.find(p=>p.left===classLeft?.id&&p.right===classRight?.id)?.value??0;
 const selectedValue=state.kerning.find(p=>p.right===target?.id)?.value??inherited;
 useEffect(()=>{setValue(String(selectedValue));},[target?.id,selectedValue]);
 const options=state.glyphs.filter(g=>g.id!==0&&(search.length>0?g.name.toLowerCase().includes(search.toLowerCase())||`#${g.id}`===search||g.codepoints.some(c=>String.fromCodePoint(c)===search):paired.has(g.id))).slice(0,24);
 const commit=async(right:number,n:number)=>{setBusy(true);try{await edit({type:'kerning',left:state.glyph,right,value:n,revision:state.revision});setError('');}catch(e){setError(String(e)==='TooLarge'?t.limit:referenceError(state.workspace.locale,e));}finally{setBusy(false);}};
 const number=Number(value);const valid=!!target&&value.trim()!==''&&Number.isInteger(number)&&number>=-32768&&number<=32767;
 const preview=()=>{const l=state.codepoints[0];const r=target?.codepoints[0];if(l===undefined||r===undefined){setError(t.missing);return;}void proofCommand(state,{type:'prepare',settings:{...state.proof.settings,kerning:true,text:String.fromCodePoint(l,r,l,r),scroll:0}}).catch(e=>setError(referenceError(state.workspace.locale,e)));};
 return <><section className="kerning-editor" aria-label={t.title}>
  <div className="kerning-heading"><strong>{t.title}</strong><span>{state.kerning.length} / {state.kerning_total}</span><span className="tool-spacer"/><span className="panel-help" role="img" tabIndex={0} title={t.help} aria-label={t.help}><Icon name="info"/></span></div>
  <div className="kerning-pair"><span className="kerning-left" title={state.name}>{state.codepoints.length?String.fromCodePoint(state.codepoints[0]):`#${state.glyph}`}</span><span>↔</span><input aria-label={t.right} placeholder={t.right} list={`kern-targets-${state.glyph}`} value={query} onChange={e=>{setQuery(e.target.value);const g=state.glyphs.find(g=>e.target.value===`#${g.id}`||e.target.value===g.name||g.codepoints.some(c=>String.fromCodePoint(c)===e.target.value));setValue(String(state.kerning.find(p=>p.right===g?.id)?.value??0));}}/><datalist id={`kern-targets-${state.glyph}`}>{options.map(g=><option key={g.id} value={`#${g.id}`}>{g.character} · {g.name}</option>)}</datalist></div>
  <div className="kerning-inherited" title={ct.pairHelp}>{ct.inherited}: {inherited}</div>
  <div className="kerning-actions"><input aria-label={t.value} title={t.value} type="number" min={-32768} max={32767} step={1} value={value} onChange={e=>setValue(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&valid&&!busy)void commit(target!.id,number);}}/><IconButton icon="check" label={t.apply} disabled={!valid||busy} onClick={()=>void commit(target!.id,number)}/><IconButton icon="eyeOff" label={ct.zero} disabled={!target||busy} onClick={()=>{if(!target)return;setBusy(true);void edit({type:'kerning_exception',left:state.glyph,right:target.id,revision:state.revision}).then(()=>setError('')).catch(e=>setError(String(e)==='TooLarge'?t.limit:referenceError(state.workspace.locale,e))).finally(()=>setBusy(false));}}/><IconButton icon="eye" label={t.preview} disabled={!target||busy} onClick={preview}/></div>
  <div className="kerning-pairs">{state.kerning.map(p=>{const g=glyphs.get(p.right);return <div className="kerning-row" key={p.right}><button title={`${g?.name} · #${p.right}`} onClick={()=>{setQuery(`#${p.right}`);setValue(String(p.value));}}><span>{g?.character||g?.name||`#${p.right}`}</span><span>{p.value>0?'+':''}{p.value}</span></button><IconButton icon="trash" label={`${t.remove} #${p.right}`} disabled={busy} onClick={()=>void commit(p.right,0)}/></div>;})}</div>
  {error&&<p className="field-error" role="alert">{error}</p>}
 </section><ClassKerning state={state} edit={edit}/></>;
}
