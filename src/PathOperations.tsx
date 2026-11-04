import {useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Icon} from './icons';
import {IconButton} from './Panel';
import {processingLabels} from './path-i18n';
import type {Edit,Snapshot} from './types';
export function PathOperations({state,edit}:{state:Snapshot;edit:(a:Edit)=>Promise<void>}){
 const t=processingLabels[state.workspace.locale];
 const [dx,setDx]=useState('0'),[dy,setDy]=useState('0'),[target,setTarget]=useState('');
 const [busy,setBusy]=useState(false),[error,setError]=useState('');
 const left=state.snippets.contours;
 const choices=state.contours.flatMap((c,i)=>c.closed&&!left.includes(i)?[i]:[]);
 const right=choices.includes(Number(target))&&target!==''?Number(target):choices[0];
 const count=state.selected_nodes.length;
 const booleanReady=left.length>0&&left.every(i=>state.contours[i]?.closed)&&right!==undefined&&!state.pen_points;
 const apply=async(operation:string)=>{if(busy)return;setBusy(true);setError('');try{await invoke('boolean_paths',{request:{epoch:state.epoch,revision:state.revision,glyph:state.glyph,left,right,operation}});}catch(e){const code=String(e);setError(code==='boolean_limit'?t.limit:code==='boolean_geometry'?t.geometry:code==='Conflict'?state.workspace.locale==='ja'?'文書が変更されました。再度実行してください。':state.workspace.locale==='zh_cn'?'文档已更改，请重试。':'The document changed. Try again.':t.failed);}finally{setBusy(false);}};
 return <div className="path-processing" aria-busy={busy}>
  <form className="path-processing-row" onSubmit={e=>{e.preventDefault();if(!busy&&count&&dx!==''&&dy!==''&&Number.isFinite(Number(dx))&&Number.isFinite(Number(dy)))void edit({type:'move_selected',dx:Number(dx),dy:Number(dy),revision:state.revision});}}>
   <IconButton icon="selectAll" label={t.all} disabled={busy||!!state.pen_points} onClick={()=>void edit({type:'select_all_nodes'})}/>
   <IconButton icon="close" label={t.clear} disabled={busy||!count} onClick={()=>void edit({type:'clear_nodes'})}/>
   <span className="path-selection-count">{count}</span>
   <input type="number" aria-label={t.dx} title={t.dx} step="any" value={dx} onChange={e=>setDx(e.target.value)}/>
   <input type="number" aria-label={t.dy} title={t.dy} step="any" value={dy} onChange={e=>setDy(e.target.value)}/>
   <button className="icon-button" type="submit" aria-label={t.move} title={t.move} disabled={busy||!count||dx===''||dy===''}><Icon name="movePoints"/></button>
  </form>
  <div className="path-processing-row"><span className="path-selection-count">A {left.length}</span><select aria-label={t.target} title={t.target} value={right??''} disabled={busy} onChange={e=>setTarget(e.target.value)}>{right===undefined&&<option value="">B —</option>}{choices.map(i=><option key={i} value={i}>B #{i+1}</option>)}</select>
   {(['union','difference','intersection','xor'] as const).map(operation=><IconButton key={operation} icon={operation} label={t[operation]} disabled={busy||!booleanReady} onClick={()=>void apply(operation)}/>)}
   <span className="panel-help" tabIndex={0} role="img" aria-label={t.help} title={t.help}><Icon name="info"/></span>
  </div>
  {error&&<div className="reference-error" role="alert">{error}</div>}
 </div>;
}
