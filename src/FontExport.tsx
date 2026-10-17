import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {IconButton} from './Panel';
import {Icon} from './icons';
import {libraryMessages,libraryError} from './library-i18n';
import {messages} from './i18n';
import type {Snapshot} from './types';
export const fontOutputFormats=[['ttf','TrueType (.ttf)'],['otf','OpenType CFF (.otf)'],['cid_otf','OpenType CID-CFF (.otf)'],['pfa','Type 1 PFA (.pfa)'],['pfb','Type 1 PFB (.pfb)'],['cid','CID Type 1 (.cid)'],['cff','CFF (.cff)'],['cid_cff','CID-CFF (.cff)'],['cmap','Unicode → CID (.cmap)']];
export function FontExport({state,onOpenChange}:{state:Snapshot;onOpenChange:(open:boolean)=>void}){
  const dialog=useRef<HTMLDialogElement>(null);
  const pending=useRef(false);
  const [family,setFamily]=useState(state.family);
  const [format,setFormat]=useState('ttf');
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState('');
  const t=libraryMessages[state.workspace.locale];
  useEffect(()=>{setFamily(state.family);setError('');},[state.epoch,state.family]);
  const save=async()=>{
    if(pending.current)return;
    pending.current=true;setBusy(true);setError('');
    try{await invoke('font_library_action',{operation:{type:'export',family,format}});dialog.current?.close();}
    catch(e){const message=String(e);const key=libraryError(message);setError(key?t[key]:`${t.failure}: ${message}`);}
    finally{pending.current=false;setBusy(false);}
  };
  return <>
    <IconButton icon="download" label={t.exportFont} onClick={()=>{setError('');dialog.current?.showModal();onOpenChange(true);}}/>
    <dialog ref={dialog} className="font-export-dialog" onClose={()=>onOpenChange(false)} onCancel={e=>{if(pending.current)e.preventDefault();}} aria-labelledby="font-export-title">
      <form onSubmit={e=>{e.preventDefault();void save();}}>
        <header><strong id="font-export-title">{t.exportFont}</strong><span className="panel-help" tabIndex={0} role="img" aria-label={t.exportHelp} title={t.exportHelp}><Icon name="info"/></span><IconButton icon="close" label={messages[state.workspace.locale].cancel} disabled={busy} onClick={()=>dialog.current?.close()}/></header>
        <input aria-label={t.family} value={family} disabled={busy} maxLength={256} onChange={e=>setFamily(e.target.value)} autoFocus/>
        <select aria-label={t.outputFormat} value={format} disabled={busy} onChange={e=>setFormat(e.target.value)}>{fontOutputFormats.map(([value,label])=><option key={value} value={value}>{label}</option>)}</select>
        {error&&<p role="alert">{error}</p>}
        <button type="submit" disabled={busy||!family.trim()}><Icon name="download"/>{busy?t.loading:t.exportFont}</button>
      </form>
    </dialog>
  </>;
}
