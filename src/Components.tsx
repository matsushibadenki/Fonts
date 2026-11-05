import {convertFileSrc} from '@tauri-apps/api/core';
import {useState} from 'react';
import {IconButton} from './Panel';import {Icon} from './icons';
import {componentLabels,componentCommand,componentError} from './component-i18n';
import {snippetCommand} from './snippet-i18n';import {referenceLabels} from './reference-i18n';
import type {Snapshot,ReferenceImage} from './types';
export function Components({state}:{state:Snapshot}){
 const t=componentLabels[state.workspace.locale],rt=referenceLabels[state.workspace.locale];
 const [query,setQuery]=useState(''),[name,setName]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const data=state.components,selected=data.definitions.find(d=>d.id===data.selected)??data.definitions[0];
 const placement=state.snippets.reference?state.snippets.placement:null;
 const blocked=busy||!!state.snippets.placement||state.proof.active||state.image_editing;
 const run=async(action:Record<string,unknown>,preview=false)=>{if(busy)return;setBusy(true);setError('');try{if(preview)await snippetCommand(state,action);else await componentCommand(state,action);}catch(e){setError(componentError(state.workspace.locale,e));}finally{setBusy(false);}};
 const normalize=(s:string)=>s.normalize('NFKC').toLocaleLowerCase();
 const instances=data.instances.filter(i=>i.definition===selected?.id);
 return <div className="snippets-panel">
  <div className="snippet-actions">
   <IconButton icon="plus" label={t.capture} disabled={blocked||!state.snippets.contours.length||!name.trim()} onClick={()=>void run({type:'capture',info:{name:name.trim(),tags:'',category:'radical',source:state.family,license:'',favorite:false}})}/>
   <IconButton icon="edit" label={t.rename} disabled={blocked||!selected||!name.trim()} onClick={()=>void run({type:'rename',id:selected!.id,info:{...selected!.info,name:name.trim()}})}/>
   <IconButton icon="save" label={t.update} disabled={blocked||!selected||!state.snippets.contours.length||state.snippets.contours.some(i=>state.contours[i]?.linked)} onClick={()=>void run({type:'update',id:selected!.id})}/>
   <IconButton icon="trash" label={t.remove} disabled={blocked||!selected||!!selected.uses} onClick={()=>void run({type:'remove',id:selected!.id})}/>
   <span className="panel-help" role="img" tabIndex={0} title={t.help} aria-label={t.help}><Icon name="info"/></span>
  </div>
  <input aria-label={t.name} title={t.name} placeholder={t.name} value={name} maxLength={256} disabled={blocked} onChange={e=>setName(e.target.value)}/>
  <label className="search"><Icon name="search"/><input aria-label={t.search} placeholder={t.search} value={query} onChange={e=>setQuery(e.target.value)}/></label>
  <div className="snippet-grid" role="listbox" aria-label={t.title}>{data.definitions.filter(d=>normalize(`${d.info.name} ${d.info.tags}`).includes(normalize(query))).map(d=><button role="option" key={d.id} aria-selected={selected?.id===d.id} className={selected?.id===d.id?'active':''} disabled={blocked} onClick={()=>{setName(d.info.name);void run({type:'choose',id:d.id});}}><img key={`${state.epoch}-${state.revision}-${d.id}`} src={convertFileSrc(`component-${state.epoch}-${state.revision}-${d.thumbnail}.png`,'thumbnail')} alt="" loading="lazy" draggable={false}/><span>{d.info.name}</span></button>)}</div>
  {!data.definitions.length&&<span className="empty">{t.empty}</span>}
  {selected&&!placement&&<div className="snippet-actions"><span className="snippet-name">{selected.info.name}</span><IconButton icon="snippets" label={t.place} disabled={blocked} onClick={()=>void run({type:'begin',id:selected.id})}/><IconButton icon="contour" label={t.source} disabled={blocked||!selected.source} onClick={()=>void run({type:'source',id:selected.id})}/><span className="path-selection-count" title={t.uses}>{selected.uses}</span></div>}
  {placement&&<div className="snippet-placement"><div className="reference-fields">{(['x','y','width','height','angle'] as const).map(field=>{const value=field==='x'?placement.center[0]:field==='y'?placement.center[1]:placement[field];return <label key={field}><span>{rt[field]}</span><input key={`${field}-${value}`} aria-label={`${t.preview} ${rt[field]}`} type="number" step="any" disabled={busy} defaultValue={Number(value.toFixed(3))} onBlur={e=>{const n=Number(e.target.value);if(!e.target.value||!Number.isFinite(n)){setError(t.InvalidGeometry);return;}if(Math.abs(n-value)<.0005)return;const p:ReferenceImage={...placement,center:[...placement.center]};if(field==='x'||field==='y')p.center[field==='x'?0:1]=n;else p[field]=n;void run({type:'transform',placement:p},true);}} onKeyDown={e=>{if(e.key==='Enter')e.currentTarget.blur();}}/></label>;})}</div><div className="snippet-actions"><IconButton icon="check" label={t.confirm} disabled={busy} onClick={()=>void run({type:'commit'},true)}/><IconButton icon="close" label={t.cancel} disabled={busy} onClick={()=>void run({type:'cancel'},true)}/></div></div>}
  {selected&&<div className="component-uses"><small>{t.uses} · {instances.length}</small>{instances.map(i=><div className="snippet-actions" key={i.id}><button className="component-use-name" title={t.usage} disabled={blocked} onClick={()=>void run({type:'usage',id:i.id})}>{i.name}</button><IconButton icon="movePoints" label={`${t.transform} · ${i.name}`} disabled={blocked} onClick={()=>void run({type:'transform',id:i.id})}/><IconButton icon="independent" label={`${t.detach} · ${i.name}`} disabled={blocked} onClick={()=>void run({type:'detach',id:i.id})}/></div>)}</div>}
  {!placement&&<details className="snippet-contours"><summary>{state.snippets.contours.length}/{state.contours.length}</summary>{state.contours.map((c,i)=><label key={c.uuid??i}><input type="checkbox" aria-label={`#${i+1}`} checked={state.snippets.contours.includes(i)} disabled={blocked||c.linked} onChange={e=>void run({type:'contour',index:i,selected:e.target.checked},true)}/><span>#{i+1}{c.linked?' ↗':''}</span></label>)}</details>}
  {error&&<div className="reference-error" role="alert" style={{whiteSpace:'pre-line'}}>{error}</div>}
 </div>;
}
