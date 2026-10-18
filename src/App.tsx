import {DocumentTabs,documentError} from './DocumentTabs';
import {DockTree} from './DockTree';
import {referenceCommand} from './ReferenceImages';
import {referenceError} from './reference-i18n';
import {Preferences} from './Preferences';
import {FontProperties} from './FontProperties';
import {fontEditMessages} from './font-edit-i18n';
import {proofCommand,proofError} from './ProofControls';
import {Recovery} from './Recovery';
import {FontExport} from './FontExport';
import {FontLibrary} from './FontLibrary';
import {libraryMessages} from './library-i18n';
import {glyphLabel,unicodeCode} from './unicode';
import {useEffect,useRef,useState} from 'react';
import {getCurrentWindow} from '@tauri-apps/api/window';
import {isTauri} from '@tauri-apps/api/core';
import {useWorkspace} from './bridge';
import {messages} from './i18n';
import type {Key} from './i18n';
import {Canvas} from './Canvas';
import {IconButton,Panel} from './Panel';
import {Icon} from './icons';
import type {Dock,Layout,Panel as PanelId,Snapshot,SplitSide} from './types';
const panelIds:PanelId[]=['glyphs','metrics','outline','snippets'];
function DropZone({dock,t,onDrop}:{dock:Dock;t:(k:Key)=>string;onDrop:(p:PanelId,d:Dock)=>void}){
  const [over,setOver]=useState(false);
  return <div className={`drop-zone drop-${dock} ${over?'over':''}`} data-dock={dock} onDragOver={e=>{e.preventDefault();e.dataTransfer.dropEffect='move';setOver(true);}} onDragLeave={()=>setOver(false)} onDrop={e=>{e.preventDefault();setOver(false);const p=e.dataTransfer.getData('text/plain') as PanelId;if(panelIds.includes(p))onDrop(p,dock);}}><Icon name={dock}/><span>{t(dock)}</span></div>;
}
export default function App(){
  const {state,error,setError,edit,layout,run,refresh}=useWorkspace();
  const [overlayOpen,setOverlayOpen]=useState(false);
  const [dragging,setDragging]=useState<PanelId|null>(null);
  const dragRef=useRef<PanelId|null>(null);
  const setDrag=(p:PanelId|null)=>{dragRef.current=p;setDragging(p);};
  const label=isTauri()?getCurrentWindow().label:'main';
  const floating=panelIds.find(p=>label===`panel-${p}`);
  const t=(key:Key)=>messages[state?.workspace.locale||'ja'][key];
  useEffect(()=>{document.documentElement.lang=state?.workspace.locale==='zh_cn'?'zh-CN':state?.workspace.locale||'ja';},[state?.workspace.locale]);
  useEffect(()=>{
    if(!state)return;
    const key=(e:KeyboardEvent)=>{
      if(e.target instanceof HTMLInputElement||e.target instanceof HTMLTextAreaElement||e.target instanceof HTMLSelectElement||(e.target instanceof HTMLElement&&e.target.isContentEditable)||e.isComposing)return;
      if((e.key==='Delete'||e.key==='Backspace')&&state.tool==='images'&&state.image_selected!==null&&!state.proof.active){e.preventDefault();void referenceCommand(state,{type:'remove',id:state.image_selected}).catch(e=>setError(referenceError(state.workspace.locale,e)));}
      if(e.key==='Escape'){setDrag(null);void edit({type:'cancel'});}
    };window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);
  },[state,edit,run]);
  const move=async(p:PanelId,d:Dock)=>{await layout({type:'dock',panel:p,dock:d});setDrag(null);};
  const actLayout=async(a:Layout)=>{await layout(a);setDrag(null);};
  if(!state)return <div className="startup"><span className="brand-mark">F</span><p>{error==='native_required'?t('startup'):error||t('loading')}</p>{error!=='native_required'&&<button onClick={()=>void refresh()}>{t('retry')}</button>}</div>;
  if(label==='font-library')return <FontLibrary state={state}/>;
  return <div className={`app ${floating?'floating-app':''}`} onPointerUp={e=>{
    if(!dragRef.current)return;
    const relative=document.elementFromPoint(e.clientX,e.clientY)?.closest<HTMLElement>('[data-relative]');
    if(relative){const panel=dragRef.current;setDrag(null);void actLayout({type:'relative',panel,target:relative.dataset.target as PanelId,side:relative.dataset.relative as SplitSide});return;}
    const hit=document.elementFromPoint(e.clientX,e.clientY)?.closest<HTMLElement>('[data-dock]');
    const dock=hit?.dataset.dock as Dock|undefined;
    const panel=dragRef.current;setDrag(null);
    if(dock)void actLayout({type:'dock',panel,dock});else void actLayout({type:'float',panel});
  }}>
    {floating?<Panel id={floating} state={state} t={t} edit={edit} layout={actLayout} floating/>:<>
      <header className="app-header">
        <span className="brand"><img className="brand-icon" src="/app-icon.svg" alt=""/>Fonts<span className="slash">/</span></span><span className="family">{state.family}{state.dirty&&<span className="dirty-dot" title={t('draft')}>●</span>}</span>
        <nav className="workflow" aria-label="Workflow">{(['draw','space','test','export'] as const).map(k=><button key={k} className={(state.proof.active?k==='test':k==='draw')?'active':''} disabled={k==='space'||k==='export'} title={k==='space'||k==='export'?t('pending'):t(k)} onClick={()=>void proofCommand(state,k==='test'?{type:'prepare',settings:state.proof.settings}:{type:'close'}).then(()=>setError('')).catch(e=>setError(proofError(state,String(e))))}>{t(k)}</button>)}</nav>
        <div className="app-actions">
          <IconButton icon="newDocument" label={t('newDocument')} onClick={()=>void run('new_project')}/>
          <IconButton icon="library" label={libraryMessages[state.workspace.locale].library} onClick={()=>void run('open_font_library')}/>
          <FontProperties state={state} onOpenChange={setOverlayOpen}/>
          <span className="action-divider" aria-hidden="true"/>
          <IconButton icon="undo" label={t('undo')} disabled={!state.can_undo} onClick={()=>void edit({type:'undo',revision:state.revision})}/>
          <IconButton icon="redo" label={t('redo')} disabled={!state.can_redo} onClick={()=>void edit({type:'redo',revision:state.revision})}/>
          <span className="action-divider" aria-hidden="true"/>
          <IconButton icon="save" label={t('save')} onClick={()=>void run('save_project')}/>
          <FontExport state={state} onOpenChange={setOverlayOpen}/>
          <Recovery locale={state.workspace.locale} onOpenChange={setOverlayOpen}/>
          <span className="action-divider" aria-hidden="true"/>
          <Preferences state={state} onOpenChange={setOverlayOpen}/>
        </div>
      </header>
      <DocumentTabs state={state} onError={setError} onNew={()=>void run('new_project')}/>
      <main className="workspace">
        <DockArea dock="left" state={state} t={t} edit={edit} layout={actLayout} onDrag={setDrag} onDrop={move} dragging={dragging}/>
        <div className="center-column"><Canvas state={state} t={t} edit={edit} dragging={!!dragging||overlayOpen} onError={setError}/>
          <DockArea dock="bottom" state={state} t={t} edit={edit} layout={actLayout} onDrag={setDrag} onDrop={move} dragging={dragging}/>
        </div>
        <DockArea dock="right" state={state} t={t} edit={edit} layout={actLayout} onDrag={setDrag} onDrop={move} dragging={dragging}/>
        {!!dragging&&<div className="drop-overlay">{(['left','right','bottom'] as const).map(d=><DropZone dock={d} key={d} t={t} onDrop={(p,d)=>void move(p,d)}/>)}</div>}
      </main>
      <footer className="statusbar"><span>{state.codepoints.length?glyphLabel(state.character):state.name} · {state.codepoints.map(unicodeCode).join(', ')}</span><span>{t('baseline')} 0 <b>·</b> {t('ascender')} {state.ascender} <b>·</b> {t('descender')} {state.descender}</span><span>{state.contours.length} {t('contours')} <b>·</b> {t(state.reviewed?'reviewed':'draft')}</span></footer>
    </>}
    {error&&<div role="alert" className="error-toast"><Icon name="info"/><span title={error}>{t('error')}: {error==='UnicodeConflict'?fontEditMessages[state.workspace.locale].conflict:error==='InvalidUnicode'?t('invalidUnicode'):error==='TooLarge'?t('unicodeLimit'):documentError(state.workspace.locale,error)??referenceError(state.workspace.locale,error)}</span><button aria-label={t('cancel')} onClick={()=>setError('')}><Icon name="close"/></button></div>}
  </div>;
}
function DockArea({dock,state,t,edit,layout,onDrag,onDrop,dragging}:{dragging:PanelId|null;dock:Dock;state:Snapshot;t:(k:Key)=>string;edit:(a:{type:string;[k:string]:unknown})=>Promise<void>;layout:(a:Layout)=>Promise<void>;onDrag:(p:PanelId|null)=>void;onDrop:(p:PanelId,d:Dock)=>Promise<void>}){
  const resize=useRef<{x:number;y:number;size:number;value:number}|null>(null);
  const [preview,setPreview]=useState<number|null>(null);
  const panels=panelIds.filter(p=>{const v=state.workspace.panels[p];return v.kind==='docked'&&v.dock===dock;});
  if(!panels.length)return null;
  const tree=state.workspace.trees[dock];
  return <aside data-dock={dock} className={`dock dock-${dock}`} style={dock==='bottom'?{height:preview??state.workspace.bottom_height}:{width:preview??(dock==='left'?state.workspace.left_width:state.workspace.right_width)}} onDragOver={e=>e.preventDefault()} onDrop={e=>{e.preventDefault();const p=e.dataTransfer.getData('text/plain') as PanelId;if(panelIds.includes(p))void onDrop(p,dock);}}>
    <div className={`dock-resizer resizer-${dock}`} role="separator" aria-label={t(dock)} aria-orientation={dock==='bottom'?'horizontal':'vertical'} tabIndex={0}
      onPointerDown={e=>{e.preventDefault();e.stopPropagation();e.currentTarget.focus();const r=e.currentTarget.parentElement!.getBoundingClientRect();const size=dock==='bottom'?r.height:r.width;resize.current={x:e.clientX,y:e.clientY,size,value:size};e.currentTarget.setPointerCapture(e.pointerId);}}
      onPointerMove={e=>{const r=resize.current;if(!r)return;const delta=dock==='left'?e.clientX-r.x:dock==='right'?r.x-e.clientX:r.y-e.clientY;const min=dock==='left'?160:dock==='right'?180:100;const max=dock==='bottom'?520:360;const value=Math.max(min,Math.min(max,r.size+delta));r.value=value;setPreview(value);}}
      onPointerUp={e=>{e.stopPropagation();if(!resize.current)return;const value=resize.current.value;resize.current=null;setPreview(null);void layout({type:'resize',dock,value});}}
      onPointerCancel={()=>{resize.current=null;setPreview(null);}}
      onKeyDown={e=>{if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(e.key))return;e.preventDefault();const size=dock==='bottom'?state.workspace.bottom_height:dock==='left'?state.workspace.left_width:state.workspace.right_width;const delta=e.key==='ArrowRight'||e.key==='ArrowDown'?10:-10;const min=dock==='left'?160:dock==='right'?180:100;const max=dock==='bottom'?520:360;void layout({type:'resize',dock,value:Math.max(min,Math.min(max,size+delta))});}}/>
    {tree&&<DockTree node={tree} dock={dock} path={[]} state={state} t={t} edit={edit} layout={layout} onDrag={onDrag} dragging={dragging}/>}
  </aside>;
}
