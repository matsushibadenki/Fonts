import {useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {IconButton} from './Panel';
import {Icon} from './icons';
import {fontEditMessages} from './font-edit-i18n';
import {libraryError,libraryMessages} from './library-i18n';
import type {FontProperties as Properties,Snapshot} from './types';
export function FontProperties({state,onOpenChange}:{state:Snapshot;onOpenChange:(v:boolean)=>void}){
 const t=fontEditMessages[state.workspace.locale];const dialog=useRef<HTMLDialogElement>(null);const pending=useRef(false);const token=useRef({epoch:0,revision:0});
 const [draft,setDraft]=useState<Properties>(state.properties);const [busy,setBusy]=useState(false);const [error,setError]=useState('');
 const save=async()=>{if(pending.current)return;pending.current=true;setBusy(true);setError('');try{await invoke('edit',{epoch:token.current.epoch,action:{type:'font_properties',properties:draft,revision:token.current.revision}});dialog.current?.close();}catch(e){const text=String(e);const key=libraryError(text);setError(text==='Conflict'?t.stale:key?libraryMessages[state.workspace.locale][key]:t.invalid);}finally{pending.current=false;setBusy(false);}};
 return <><IconButton icon="info" label={t.info} onClick={()=>{token.current={epoch:state.epoch,revision:state.revision};setDraft(structuredClone(state.properties));setError('');dialog.current?.showModal();onOpenChange(true);}}/>
 <dialog ref={dialog} className="font-properties-dialog" aria-labelledby="font-properties-title" onClose={()=>onOpenChange(false)} onCancel={e=>{if(pending.current)e.preventDefault();}}><form onSubmit={e=>{e.preventDefault();void save();}}>
 <header><strong id="font-properties-title">{t.info}</strong><span className="panel-help" tabIndex={0} role="img" aria-label={t.infoHelp} title={t.infoHelp}><Icon name="info"/></span><IconButton icon="close" label={t.close} disabled={busy} onClick={()=>dialog.current?.close()}/></header>
 <label>{t.family}<input required maxLength={256} aria-label={t.family} value={draft.family} disabled={busy} onChange={e=>setDraft({...draft,family:e.target.value})}/></label>
 <label>{t.style}<input maxLength={256} aria-label={t.style} value={draft.metadata.style} disabled={busy} onChange={e=>setDraft({...draft,metadata:{...draft.metadata,style:e.target.value}})}/></label>
 <div className="font-property-numbers">{(['weight','width','italic_angle'] as const).map(k=><label key={k}>{t[k==='italic_angle'?'angle':k]}<input type="number" required min={k==='italic_angle'?-90:0} max={k==='weight'?1000:k==='width'?9:90} step={k==='italic_angle'?.1:1} value={draft.metadata[k]} disabled={busy} onChange={e=>setDraft({...draft,metadata:{...draft.metadata,[k]:Number(e.target.value)}})}/></label>)}</div>
 <div className="font-property-flags">{(['bold','italic'] as const).map(k=><label key={k}><input type="checkbox" checked={draft.metadata[k]} disabled={busy} onChange={e=>setDraft({...draft,metadata:{...draft.metadata,[k]:e.target.checked}})}/>{t[k]}</label>)}</div>
 {(['copyright','license','license_url'] as const).map(k=><label key={k}>{t[k==='license_url'?'url':k]}<textarea maxLength={8192} aria-label={t[k==='license_url'?'url':k]} value={draft.metadata[k]} disabled={busy} onChange={e=>setDraft({...draft,metadata:{...draft.metadata,[k]:e.target.value}})}/></label>)}
 {error&&<p role="alert">{error}</p>}<button type="submit" disabled={busy||!draft.family.trim()}>{t.apply}</button>
 </form></dialog></>;
}
