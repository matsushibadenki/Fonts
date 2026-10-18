import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {IconButton} from './Panel';
import type {Locale,Snapshot} from './types';
export const documentLabels={
 en:{tabs:'Documents',close:'Close document',new:'New document',dirty:'Unsaved changes',limit:'Up to 16 documents can be open. Close a document first.',path:'This path is used by another open document. Choose another filename.',draft:'Finish or cancel the current edit before switching documents.',missing:'This document has already been closed.',busy:'Wait for the current file operation to finish.'},
 ja:{tabs:'文書',close:'文書を閉じる',new:'新しい文書',dirty:'未保存の変更',limit:'同時に開ける文書は16件までです。先に文書を閉じてください。',path:'別の開いている文書がこの保存先を使っています。別のファイル名を選択してください。',draft:'編集中の操作を確定または取り消してから文書を切り替えてください。',missing:'この文書はすでに閉じられています。',busy:'ファイル処理の完了を待ってください。'},
 zh_cn:{tabs:'文档',close:'关闭文档',new:'新建文档',dirty:'未保存的更改',limit:'最多同时打开16个文档，请先关闭一个文档。',path:'另一个已打开文档正在使用此路径，请选择其他文件名。',draft:'切换文档前请确认或取消当前编辑。',missing:'此文档已关闭。',busy:'请等待当前文件操作完成。'}
};
export function documentError(locale:Locale,error:string):string|null{const t=documentLabels[locale];return ({document_limit:t.limit,document_path_in_use:t.path,edit_in_progress:t.draft,document_not_found:t.missing,file_operation_in_progress:t.busy} as Record<string,string>)[error]??null;}
export function DocumentTabs({state,onError,onNew}:{state:Snapshot;onError:(e:string)=>void;onNew:()=>void}){
 const pending=useRef(false);const [busy,setBusy]=useState(false);const t=documentLabels[state.workspace.locale];
 useEffect(()=>{document.getElementById(`document-tab-${state.document_id}`)?.scrollIntoView({block:'nearest',inline:'nearest'});},[state.document_id]);
 const act=async(id:number,close:boolean)=>{if(pending.current)return;pending.current=true;setBusy(true);try{await invoke('document_action',{id,close});onError('');}catch(e){onError(String(e));}finally{pending.current=false;setBusy(false);}};
 return <nav className="document-bar" aria-label={t.tabs}>
  <div className="document-tabs" role="tablist" aria-label={t.tabs}>{state.documents.map(d=><div className={`document-tab ${d.id===state.document_id?'active':''}`} key={d.id}>
   <button id={`document-tab-${d.id}`} role="tab" tabIndex={d.id===state.document_id?0:-1} aria-selected={d.id===state.document_id} title={d.path??d.family} aria-disabled={busy} onClick={()=>void act(d.id,false)} onKeyDown={e=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();const index=state.documents.findIndex(x=>x.id===d.id);const next=e.key==='Home'?0:e.key==='End'?state.documents.length-1:(index+(e.key==='ArrowLeft'?-1:1)+state.documents.length)%state.documents.length;void act(state.documents[next].id,false).then(()=>document.getElementById(`document-tab-${state.documents[next].id}`)?.focus());}}><span>{d.path?.split(/[\\/]/).pop()??`${d.family} · ${d.id}`}</span>{d.dirty&&<i title={t.dirty} aria-label={t.dirty}>●</i>}</button>
   <IconButton icon="close" label={`${t.close}: ${d.family} · ${d.id}`} disabled={busy} onClick={()=>void act(d.id,true)}/>
  </div>)}</div>
  <IconButton icon="plus" label={t.new} disabled={busy} onClick={onNew}/>
 </nav>;
}
