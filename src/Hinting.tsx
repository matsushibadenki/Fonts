import {useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {IconButton} from './Panel';
import {Icon} from './icons';
import {libraryError,libraryMessages} from './library-i18n';
import type {Snapshot} from './types';
import {hintingLabels} from './hinting-i18n';

type Info={family:string;extension:string;true_type:boolean;instructed_glyphs:number;instruction_bytes:number;tables:string[]};
type Report={path:string;bytes:number;verified:boolean};
export function Hinting({id,state,disabled}:{id:number;state:Snapshot;disabled:boolean}){
 const t=hintingLabels[state.workspace.locale];const dialog=useRef<HTMLDialogElement>(null);const pending=useRef(false);
 const [info,setInfo]=useState<Info|null>(null);const [remove,setRemove]=useState(false);const [busy,setBusy]=useState(false);const [error,setError]=useState('');const [report,setReport]=useState<Report|null>(null);
 const fail=(e:unknown)=>{const key=libraryError(String(e));setError(key?libraryMessages[state.workspace.locale][key]:t.error);};
 const open=async()=>{if(pending.current)return;pending.current=true;setBusy(true);setInfo(null);setReport(null);setError('');setRemove(false);dialog.current?.showModal();try{setInfo(await invoke<Info>('font_hinting_info',{id}));}catch(e){fail(e);}finally{pending.current=false;setBusy(false);}};
 const save=async()=>{if(pending.current||!info)return;pending.current=true;setBusy(true);setError('');setReport(null);try{setReport(await invoke<Report|null>('font_hinting_export',{id,remove}));}catch(e){fail(e);}finally{pending.current=false;setBusy(false);}};
 return <><IconButton icon="hinting" label={t.title} disabled={disabled} onClick={()=>void open()}/><dialog ref={dialog} className="font-export-dialog hinting-dialog" aria-label={t.title} onCancel={e=>{if(pending.current)e.preventDefault();}}><form onSubmit={e=>{e.preventDefault();void save();}}><header><strong>{t.title}</strong><span className="panel-help" tabIndex={0} title={t.help} aria-label={t.help}><Icon name="info"/></span><IconButton icon="close" label={t.close} disabled={busy} onClick={()=>dialog.current?.close()}/></header>{info&&<><strong>{info.family}</strong><small>{info.true_type?`${t.glyphs}: ${info.instructed_glyphs.toLocaleString()} · ${t.bytes}: ${info.instruction_bytes.toLocaleString()}`:t.postscript}</small>{info.true_type&&<small>{info.tables.join(' · ')||(!info.instruction_bytes?t.none:'glyf')}</small>}<select aria-label={t.title} value={remove?'remove':'preserve'} disabled={busy} onChange={e=>{setRemove(e.target.value==='remove');setReport(null);}}><option value="preserve">{t.preserve}</option>{info.true_type&&<option value="remove">{t.remove}</option>}</select></>}{report&&<div className="woff2-report" role="status"><strong>{t.verified}</strong><span title={report.path}>{report.path}</span><small>{report.bytes.toLocaleString()} B</small></div>}{error&&<p role="alert">{error}</p>}<button type="submit" disabled={busy||!info}><Icon name="download"/>{busy?libraryMessages[state.workspace.locale].loading:t.save}</button></form></dialog></>;
}
