import {useCallback,useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import {IconButton} from './Panel';
import {Icon} from './icons';
import type {Locale} from './types';
const labels={
 en:{title:'Recovery',restore:'Recover as a new project',discard:'Discard recovery copy',close:'Close',empty:'No recovery copies',ready:'Autosave enabled',saved:'Recovery copy saved',error:'Recovery operation failed. Existing copies are kept.',invalid:'Unreadable recovery copies',busy:'Working…',help:'Committed edits are saved separately after a short pause, or attempted about every 30 seconds during continuous editing. Original projects are never overwritten. In-progress strokes and drags are excluded. Recovery opens an unsaved project.'},
 ja:{title:'復旧',restore:'新しいプロジェクトとして復旧',discard:'復旧用コピーを破棄',close:'閉じる',empty:'復旧候補はありません',ready:'自動保存は有効です',saved:'復旧用コピーを保存済み',error:'復旧処理に失敗しました。既存のコピーは保持しています。',invalid:'読み取れない復旧候補',busy:'処理中…',help:'確定した編集を、操作が落ち着いてから別ファイルへ自動保存します。確定した編集が続く間は約30秒ごとに保存を試みます。元のプロジェクトは上書きせず、未確定のペン入力やドラッグは対象外です。復旧すると未保存の新しいプロジェクトとして開きます。'},
 zh_cn:{title:'恢复',restore:'恢复为新项目',discard:'丢弃恢复副本',close:'关闭',empty:'没有恢复副本',ready:'自动保存已启用',saved:'恢复副本已保存',error:'恢复操作失败。已保留现有副本。',invalid:'无法读取的恢复副本',busy:'处理中…',help:'停止操作片刻后，已确认的编辑会自动保存为独立副本；连续编辑时约每30秒尝试保存。不会覆盖原项目。尚未确认的钢笔输入和拖动不包含在内。恢复后将打开一个未保存的新项目。'}
};
type Candidate={id:string;family:string;updated:number;origin:string|null};
type Status={candidates:Candidate[];invalid:number;updated:number|null;error:string|null};
export function Recovery({locale,onOpenChange}:{locale:Locale;onOpenChange:(open:boolean)=>void}){
 const t=labels[locale];const dialog=useRef<HTMLDialogElement>(null);const first=useRef(true);const pending=useRef(false);const generation=useRef(0);
 const [data,setData]=useState<Status|null>(null);const [busy,setBusy]=useState(false);const [error,setError]=useState('');
 const refresh=useCallback(async()=>{const n=++generation.current;try{const s=await invoke<Status>('recovery_query');if(n!==generation.current)return;setData(s);if(first.current){first.current=false;if(s.candidates.length||s.invalid){dialog.current?.showModal();onOpenChange(true);}}}catch(e){if(n===generation.current)setError(String(e));}},[onOpenChange]);
 useEffect(()=>{let disposed=false;let off:undefined|(()=>void);void listen('recovery-changed',()=>void refresh()).then(f=>{if(disposed)f();else{off=f;void refresh();}}).catch(e=>setError(String(e)));return()=>{disposed=true;generation.current++;off?.();};},[refresh]);
 const act=async(id:string,restore:boolean)=>{if(pending.current)return;pending.current=true;setBusy(true);setError('');try{const changed=await invoke<boolean>('recovery_action',{id,restore});await refresh();if(restore&&changed)dialog.current?.close();}catch(e){setError(String(e));}finally{pending.current=false;setBusy(false);}};
 const time=(n:number)=>new Date(n*1000).toLocaleString(locale==='zh_cn'?'zh-CN':locale);
 const failed=error||data?.error;
 return <>
  <span className="recovery-indicator"><IconButton icon="recovery" label={failed?t.error:data?.candidates.length?`${t.title} (${data.candidates.length})`:data?.updated?`${t.saved} · ${time(data.updated)}`:t.ready} onClick={()=>{void refresh();dialog.current?.showModal();onOpenChange(true);}}/>{(failed||!!data?.candidates.length)&&<i aria-hidden="true"/>}</span>
  <dialog ref={dialog} className="recovery-dialog" onClose={()=>onOpenChange(false)} aria-labelledby="recovery-title" onCancel={e=>{if(pending.current)e.preventDefault();}}>
   <header><strong id="recovery-title">{t.title}</strong><span className="panel-help" tabIndex={0} title={t.help} aria-label={t.help} role="img"><Icon name="info"/></span><IconButton icon="close" label={t.close} disabled={busy} onClick={()=>dialog.current?.close()}/></header>
   {failed&&<p role="alert" title={failed}>{t.error}</p>}
   {!!data?.invalid&&<p role="alert">{t.invalid}: {data.invalid}</p>}
   {!data?.candidates.length&&<p>{t.empty}</p>}
   <div className="recovery-list">{data?.candidates.map(c=><div className="recovery-row" key={c.id}><div title={c.origin||undefined}><strong>{c.family}</strong><small>{c.origin?.split(/[\\/]/).pop()??''} · {time(c.updated)}</small></div><IconButton icon="undo" label={t.restore} disabled={busy} onClick={()=>void act(c.id,true)}/><IconButton icon="close" label={t.discard} disabled={busy} onClick={()=>void act(c.id,false)}/></div>)}</div>
   <small role="status">{busy?t.busy:data?.updated?`${t.saved} · ${time(data.updated)}`:t.ready}</small>
  </dialog>
 </>;
}
