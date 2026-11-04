import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {IconButton} from './Panel';
import type {Snapshot} from './types';
const labels={en:{title:'Font name inspection',clear:'Clear history',choosing:'Choose a folder…',discovering:'Finding fonts',scanning:'Inspecting fonts',repairing:'Saving repaired copies',clearing:'Clearing history',complete:'Complete',scan:'Choose folder',fix:'Save repaired copies',close:'Close',help:'Checks PostScript and CFF names in OTF fonts. Repaired copies are saved in Fonts-NameFixed beside each source. Original files are preserved. Collections and WOFF2 are not repaired.',matched:'Matched',mismatch:'Mismatch',unsupported:'Not applicable',invalid:'Invalid',fixed:'Copy saved',failed:'Failed (file changed, output exists or cannot be written)',limit:'Scan limit reached; results are partial.',error:'Could not inspect or repair fonts.'},ja:{title:'フォント名称の検査',clear:'履歴を消去',choosing:'フォルダーを選択中…',discovering:'フォントを検索中',scanning:'フォントを検査中',repairing:'修正コピーを保存中',clearing:'履歴を消去中',complete:'完了',scan:'フォルダーを選択',fix:'修正コピーを保存',close:'閉じる',help:'OTFのPostScript名とCFF名を検査します。修正コピーは各元ファイルと同じ場所のFonts-NameFixedフォルダーへ保存し、元ファイルは残します。コレクション・WOFF2は修正対象外です。',matched:'一致',mismatch:'不一致',unsupported:'対象外',invalid:'不正なフォント',fixed:'コピー保存済み',failed:'失敗（元ファイル変更・保存先の重複・書込不可）',limit:'検査上限に達したため、一部の結果です。',error:'フォントを検査・修正できませんでした。'},zh_cn:{title:'字体名称检查',clear:'清除历史记录',choosing:'正在选择文件夹…',discovering:'正在查找字体',scanning:'正在检查字体',repairing:'正在保存修复副本',clearing:'正在清除历史记录',complete:'完成',scan:'选择文件夹',fix:'保存修复副本',close:'关闭',help:'检查 OTF 的 PostScript 名称和 CFF 名称。修复副本保存在原文件旁的 Fonts-NameFixed 文件夹中，保留原文件。不修复字体集合或 WOFF2。',matched:'一致',mismatch:'不一致',unsupported:'不适用',invalid:'字体无效',fixed:'已保存副本',failed:'失败（文件已更改、输出已存在或无法写入）',limit:'已达到扫描限制，结果不完整。',error:'无法检查或修复字体。'}};
type Report={generation:number;truncated:boolean;rows:{path:string;postscript:string;cff:string;status:'matched'|'mismatch'|'unsupported'|'invalid'|'fixed'|'failed'}[]};
type Progress={running:boolean;phase:'choosing'|'discovering'|'scanning'|'repairing'|'clearing'|'complete';completed:number;total:number|null;discovered:number;current:string|null};
export function FontNames({state,disabled}:{state:Snapshot;disabled:boolean}){
 const t=labels[state.workspace.locale];
 const dialog=useRef<HTMLDialogElement>(null);
 const pending=useRef(false);
 const [busy,setBusy]=useState(false);
 const [report,setReport]=useState<Report|null>(null);
 const [error,setError]=useState('');
 const [progress,setProgress]=useState<Progress|null>(null);
 useEffect(()=>{
  if(!busy)return;
  let alive=true;let polling=false;
  const poll=async()=>{if(polling)return;polling=true;try{const p=await invoke<Progress>('font_names_progress');if(alive&&p.running)setProgress(p);}catch{/* The operation itself reports errors. */}finally{polling=false}};
  void poll();const timer=setInterval(()=>void poll(),200);
  return()=>{alive=false;clearInterval(timer)};
 },[busy]);
 const act=async(action:'scan'|'repair'|'clear')=>{
  if(pending.current)return;
  pending.current=true;setBusy(true);setError('');
  setProgress({running:true,phase:action==='scan'?'choosing':action==='repair'?'repairing':'clearing',completed:0,total:null,discovered:0,current:null});
  try{
   if(action==='clear'){await invoke('font_names_clear');setReport(null);}
   else{const r=await invoke<Report|null>(action==='repair'?'font_names_repair':'font_names_scan',action==='repair'?{generation:report?.generation}:{});if(r)setReport(r);}
  }catch{setError(t.error)}finally{pending.current=false;setBusy(false);setProgress(null)}
 };
 return <><IconButton icon="check" label={t.title} disabled={disabled} onClick={()=>dialog.current?.showModal()}/>
 <dialog ref={dialog} className="font-export-dialog" aria-label={t.title} aria-busy={busy} onCancel={e=>{if(busy)e.preventDefault()}}>
  <header><strong>{t.title}</strong><span className="panel-help" title={t.help} aria-label={t.help} tabIndex={0}>ⓘ</span><IconButton icon="trash" label={t.clear} disabled={busy||(!report&&!error)} onClick={()=>void act('clear')}/><IconButton icon="close" label={t.close} disabled={busy} onClick={()=>dialog.current?.close()}/></header>
  <button disabled={busy} onClick={()=>void act('scan')}>{t.scan}</button>
  {busy&&progress&&<div className="font-name-progress" role="status" aria-live="polite">
   <div><span>{t[progress.phase]}</span><span>{progress.total===null?progress.discovered:`${progress.completed} / ${progress.total}`}</span></div>
   <progress aria-label={t[progress.phase]} max={Math.max(1,progress.total??1)} value={progress.total===null?undefined:progress.completed}/>
   {progress.current&&<small title={progress.current}>{progress.current.split(/[\\/]/).pop()}</small>}
  </div>}
  {report&&<><div role="status">{report.rows.length} · {t.mismatch}: {report.rows.filter(r=>r.status==='mismatch').length}</div>
   <div style={{maxHeight:'45vh',overflow:'auto'}}>{report.rows.map(r=><div key={r.path} style={{padding:'8px 0',borderBottom:'1px solid #ddd'}}><strong title={r.path}>{r.path.split('/').pop()}</strong><small style={{display:'block',overflowWrap:'anywhere'}}>{r.cff} → {r.postscript}</small><span>{t[r.status]}</span></div>)}</div>
   {report.truncated&&<p role="status">{t.limit}</p>}<button disabled={busy||!report.rows.some(r=>r.status==='mismatch')} onClick={()=>void act('repair')}>{t.fix}</button>
  </>}{error&&<p role="alert">{error}</p>}
 </dialog></>;
}
