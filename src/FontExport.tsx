import {hintingLabels} from './hinting-i18n';
import {woff2Labels} from './woff2-i18n';
import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {IconButton} from './Panel';
import {Icon} from './icons';
import {libraryMessages,libraryError} from './library-i18n';
import {messages} from './i18n';
import type {Snapshot} from './types';
export const fontOutputFormats=[['woff2','Web font (.woff2)'],['ttf','TrueType (.ttf)'],['otf','OpenType CFF (.otf)'],['cid_otf','OpenType CID-CFF (.otf)'],['pfa','Type 1 PFA (.pfa)'],['pfb','Type 1 PFB (.pfb)'],['cid','CID Type 1 (.cid)'],['cff','CFF (.cff)'],['cid_cff','CID-CFF (.cff)'],['cmap','Unicode → CID (.cmap)']];
export function FontExport({state,onOpenChange}:{state:Snapshot;onOpenChange:(open:boolean)=>void}){
  const dialog=useRef<HTMLDialogElement>(null);
  const pending=useRef(false);
  const [family,setFamily]=useState(state.family);
  const [format,setFormat]=useState('ttf');
  const [outline,setOutline]=useState('true_type');
  const [quality,setQuality]=useState(9);
  const [report,setReport]=useState<{path:string;bytes:number;verified:boolean}|null>(null);
  const w=woff2Labels[state.workspace.locale];
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState('');
  const t=libraryMessages[state.workspace.locale];
  useEffect(()=>{setFamily(state.family);setError('');setReport(null);},[state.epoch,state.family]);
  const save=async()=>{
    if(pending.current)return;
    pending.current=true;setBusy(true);setError('');setReport(null);
    try{const result=await invoke<{path:string;bytes:number;verified:boolean}|null>('font_library_action',{operation:{type:'export',family,format,woff2:{outline,quality}}});if(result?.verified)setReport(result);else if(result)dialog.current?.close();}
    catch(e){const message=String(e);const key=libraryError(message);setError(message==='woff2_invalid'?w.invalid:message==='woff2_options_invalid'?w.options:key?t[key]:`${t.failure}: ${message}`);}
    finally{pending.current=false;setBusy(false);}
  };
  return <>
    <IconButton icon="download" label={t.exportFont} onClick={()=>{setError('');setReport(null);dialog.current?.showModal();onOpenChange(true);}}/>
    <dialog ref={dialog} className="font-export-dialog" onClose={()=>onOpenChange(false)} onCancel={e=>{if(pending.current)e.preventDefault();}} aria-labelledby="font-export-title">
      <form onSubmit={e=>{e.preventDefault();void save();}}>
        <header><strong id="font-export-title">{t.exportFont}</strong><span className="panel-help" tabIndex={0} role="img" aria-label={t.exportHelp} title={t.exportHelp}><Icon name="info"/></span><IconButton icon="close" label={messages[state.workspace.locale].cancel} disabled={busy} onClick={()=>dialog.current?.close()}/></header>
        <input aria-label={t.family} value={family} disabled={busy} maxLength={256} onChange={e=>{setFamily(e.target.value);setReport(null);}} autoFocus/>
        <select aria-label={t.outputFormat} value={format} disabled={busy} onChange={e=>{setFormat(e.target.value);setReport(null);}}>{fontOutputFormats.map(([value,label])=><option key={value} value={value}>{value==='woff2'?w.label:label}</option>)}</select>
        {format==='woff2'&&<div className="woff2-options">
          <label>{w.outline}<select aria-label={w.outline} disabled={busy} value={outline} onChange={e=>{setOutline(e.target.value);setReport(null);}}><option value="true_type">TrueType</option><option value="cff">CFF</option></select></label>
          <label>{w.compression}<select aria-label={w.compression} disabled={busy} value={quality} onChange={e=>{setQuality(Number(e.target.value));setReport(null);}}>{([[1,w.fast],[5,w.balanced],[9,w.small],[11,w.maximum]] as const).map(([v,label])=><option key={v} value={v}>{label}</option>)}</select></label>
          <span className="panel-help" role="img" tabIndex={0} title={w.help} aria-label={w.help}><Icon name="info"/></span>
        </div>}
        {format!=='cmap'&&<small className="hinting-policy" title={hintingLabels[state.workspace.locale].projectHelp}><Icon name="hinting"/> {hintingLabels[state.workspace.locale].project} <span className="panel-help" tabIndex={0} title={hintingLabels[state.workspace.locale].projectHelp} aria-label={hintingLabels[state.workspace.locale].projectHelp}><Icon name="info"/></span></small>}
        {report&&<div className="woff2-report" role="status"><strong>{format==='woff2'?w.verified:({en:'Font file verified',ja:'フォントファイルの検証が完了しました',zh_cn:'字体文件验证完成'}[state.workspace.locale])}</strong><span title={report.path}>{report.path}</span><small>{report.bytes.toLocaleString()} B</small></div>}
        {error&&<p role="alert">{error}</p>}
        <button type="submit" disabled={busy||!family.trim()}><Icon name="download"/>{busy?t.loading:t.exportFont}</button>
      </form>
    </dialog>
  </>;
}
