import {useCallback,useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import {IconButton} from './Panel';
import {Icon} from './icons';
import {libraryMessages,libraryError} from './library-i18n';
import type {Locale} from './types';
export const batchLabels={
 en:{shortConvert:'Batch convert',convert:'Convert selection to folder',selected:'Selected',page:'Select this page',clear:'Clear selection',cancelling:'Cancelling…',cancel:'Cancel batch',cancelled:'Cancelled',success:'Succeeded',failed:'Failed',results:'Conversion results',limit:'Select 1–256 fonts.',help:'Checkboxes, Ctrl/Cmd-click and Shift-click select multiple faces. Selection persists across pages and searches. Up to 256 faces per batch. Batch output goes into a new subfolder; existing files are never overwritten. Cancel stops between fonts.'},
 ja:{shortConvert:'一括変換',convert:'選択したフォントをフォルダーへ一括変換',selected:'選択',page:'このページを選択',clear:'選択を解除',cancelling:'中止処理中…',cancel:'一括変換を中止',cancelled:'中止しました',success:'成功',failed:'失敗',results:'変換結果',limit:'1〜256書体を選択してください。',help:'チェックボックス、⌘/Ctrlクリック、Shiftクリックで複数選択できます。ページ移動・検索後も選択を保持します。1回256書体までです。一括出力は新しいサブフォルダーに保存し、既存ファイルを上書きしません。中止は書体の処理の合間に行います。'},
 zh_cn:{shortConvert:'批量转换',convert:'将所选字体批量转换到文件夹',selected:'已选',page:'选择本页',clear:'清除选择',cancelling:'正在取消…',cancel:'取消批量转换',cancelled:'已取消',success:'成功',failed:'失败',results:'转换结果',limit:'请选择1–256款字体。',help:'使用复选框、Ctrl/Cmd点击或Shift点击多选。跨页和搜索后保留选择，每批最多256款。批量输出保存到新的子文件夹，不覆盖现有文件。取消操作在各字体处理之间生效。'}
};
type Status={running:boolean;cancelled:boolean;cancel_requested:boolean;total:number;completed:number;current:string;directory:string|null;results:{source:string;family:string;style:string;index:number;path:string|null;error:string|null}[]};
export function BatchConversion({ids,format,locale,onRunning,disabled}:{ids:number[];format:string;locale:Locale;onRunning:(v:boolean)=>void;disabled:boolean}){
 const t=batchLabels[locale];const [status,setStatus]=useState<Status|null>(null);const [pending,setPending]=useState(false);const [error,setError]=useState('');const guard=useRef(false);const generation=useRef(0);
 const refresh=useCallback(async()=>{const n=++generation.current;const s=await invoke<Status>('font_batch_query');if(n===generation.current)setStatus(s);},[]);
 useEffect(()=>{let disposed=false;let off:undefined|(()=>void);void listen('font-batch-changed',()=>void refresh().catch(e=>setError(String(e)))).then(f=>{if(disposed)f();else{off=f;void refresh().catch(e=>setError(String(e)));}}).catch(e=>setError(String(e)));return()=>{disposed=true;generation.current++;off?.();};},[refresh]);
 useEffect(()=>{onRunning(pending||!!status?.running);},[pending,status?.running,onRunning]);
 const message=(error:string)=>{const key=libraryError(error);return key?libraryMessages[locale][key]:error==='font_batch_limit'?t.limit:error;};
 const start=async()=>{if(guard.current)return;guard.current=true;setPending(true);setError('');try{await invoke('font_batch_start',{ids,format});await refresh();}catch(e){setError(message(String(e)));}finally{guard.current=false;setPending(false);}};
 const ok=status?.results.filter(r=>!r.error).length||0;const failed=status?.results.filter(r=>r.error).length||0;
 return <div className="batch-conversion"><button className="library-output-button" title={t.convert} aria-label={t.convert} disabled={disabled||pending||!!status?.running||ids.length===0||ids.length>256} onClick={()=>void start()}><Icon name="folder"/>{t.shortConvert}</button>
 {!status?.running&&ids.length>0&&<span title={t.selected}>{ids.length}</span>}
 {status?.running&&<><progress aria-label={t.convert} max={status.total} value={status.completed}/><span title={status.current}>{status.completed}/{status.total} {status.cancel_requested?t.cancelling:status.current}</span><IconButton icon="close" disabled={status.cancel_requested} label={status.cancel_requested?t.cancelling:t.cancel} onClick={()=>void invoke('font_batch_cancel').catch(e=>setError(String(e)))}/></>}
 {!status?.running&&!!status?.total&&<details><summary>{status.cancelled?`${t.cancelled} · `:''}{t.success}: {ok} · {t.failed}: {failed}</summary><div className="batch-results" role="region" aria-label={t.results}><strong>{status.directory}</strong>{status.results.map((r,i)=><div key={i} title={r.path||r.error||''}><span title={r.source}>{r.family} {r.style} #{r.index}</span><small>{r.source}</small><small>{r.error?`${t.failed}: ${message(r.error)}`:r.path}</small></div>)}</div></details>}
 {ids.length>256&&<span role="status">{t.limit}</span>}{error&&<span role="alert">{error}</span>}
 </div>;
}
