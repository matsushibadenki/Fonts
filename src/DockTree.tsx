import {useRef,useState} from 'react';
import {Panel} from './Panel';
import {Icon} from './icons';
import {splitOptions} from './PanelArrangement';
import type {DockNode,Dock,Snapshot,Edit,Layout,Panel as PanelId} from './types';
import type {Key} from './i18n';
interface Props {node:DockNode;dock:Dock;path:number[];state:Snapshot;t:(k:Key)=>string;edit:(a:Edit)=>Promise<void>;layout:(a:Layout)=>Promise<void>;onDrag:(p:PanelId|null)=>void;dragging:PanelId|null}
function visible(node:DockNode,s:Snapshot):boolean{return node.kind==='tabs'?node.panels.some(p=>s.workspace.panels[p].kind==='docked'):visible(node.first,s)||visible(node.second,s);}
export function DockTree(props:Props){
  const {node,dock,path,state,t,edit,layout,onDrag,dragging}=props;
  const [preview,setPreview]=useState<number|null>(null);
  const resize=useRef<{start:number;extent:number;ratio:number;value:number}|null>(null);
  if(!visible(node,state))return null;
  if(node.kind==='tabs'){
    const panels=node.panels.filter(p=>state.workspace.panels[p].kind==='docked');
    const active=panels.includes(node.active)?node.active:panels[0];
    return <div className="dock-leaf">
      {panels.length>1&&<div className="panel-tabs" role="tablist">{panels.map(p=><button role="tab" aria-selected={p===active} className={p===active?'active':''} key={p} onClick={()=>void layout({type:'activate',panel:p})}>{t(p)}</button>)}</div>}
      <Panel key={`${state.document_id}-${active}`} id={active} state={state} t={t} edit={edit} layout={layout} onDrag={onDrag}/>
      {dragging&&dragging!==active&&<div className="leaf-drop-targets">{splitOptions.map(o=><div key={o.side} className={`leaf-drop leaf-drop-${o.side}`} data-relative={o.side} data-target={active} title={t(o.key)}><Icon name={o.icon}/></div>)}</div>}
    </div>;
  }
  const first=visible(node.first,state),second=visible(node.second,state);
  if(!first||!second)return <DockTree {...props} node={first?node.first:node.second} path={[...path,first?0:1]}/>;
  const horizontal=node.axis==='horizontal';
  const ratio=preview??node.ratio;
  const commit=(value:number)=>void layout({type:'resize_split',dock,path,value});
  return <div className={`dock-split split-${node.axis}`}>
    <div className="split-child" style={{flex:`${ratio} 1 0`}}><DockTree {...props} node={node.first} path={[...path,0]}/></div>
    <div className="split-resizer" role="separator" tabIndex={0} aria-label={t('splitResize')} aria-orientation={horizontal?'vertical':'horizontal'} aria-valuemin={15} aria-valuemax={85} aria-valuenow={Math.round(ratio*100)}
      onPointerDown={e=>{e.preventDefault();e.stopPropagation();e.currentTarget.focus();const r=e.currentTarget.parentElement!.getBoundingClientRect();resize.current={start:horizontal?e.clientX:e.clientY,extent:(horizontal?r.width:r.height)-5,ratio:node.ratio,value:node.ratio};e.currentTarget.setPointerCapture(e.pointerId);}}
      onPointerMove={e=>{const r=resize.current;if(!r)return;r.value=Math.max(.15,Math.min(.85,r.ratio+((horizontal?e.clientX:e.clientY)-r.start)/Math.max(1,r.extent)));setPreview(r.value);}}
      onPointerUp={e=>{e.stopPropagation();const r=resize.current;resize.current=null;setPreview(null);if(r)commit(r.value);}}
      onPointerCancel={()=>{resize.current=null;setPreview(null);}}
      onKeyDown={e=>{const keys=horizontal?['ArrowLeft','ArrowRight']:['ArrowUp','ArrowDown'];if(!keys.includes(e.key))return;e.preventDefault();e.stopPropagation();commit(Math.max(.15,Math.min(.85,node.ratio+(e.key===keys[0]?-.05:.05))));}}/>
    <div className="split-child" style={{flex:`${1-ratio} 1 0`}}><DockTree {...props} node={node.second} path={[...path,1]}/></div>
  </div>;
}
