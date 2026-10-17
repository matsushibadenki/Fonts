import {snippetCommand,snippetLabels} from './snippet-i18n';
import {placeReference} from './ReferenceImages';
import {referenceLabels,referenceError} from './reference-i18n';
import {ProofControls,proofCommand,proofLabels,proofError} from './ProofControls';
import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {IconButton} from './Panel';
import {Icon} from './icons';
import type {Edit,Snapshot} from './types';
import type {Key} from './i18n';
export function Canvas({state,t,edit,dragging,onError}:{state:Snapshot;t:(k:Key)=>string;edit:(a:Edit)=>Promise<void>;dragging:boolean;onError:(s:string)=>void}){
  const ref=useRef<HTMLDivElement>(null);
  const [gpu,setGpu]=useState('');
  const [importing,setImporting]=useState(false);
  const imageText=referenceLabels[state.workspace.locale];
  const pointerContext=useRef<{epoch:number;revision:number;glyph:number;token:string}|null>(null);
  const queue=useRef(Promise.resolve());
  const latest=useRef<{x:number;y:number;shift:boolean}|null>(null);
  const frame=useRef(0);
  const captured=useRef(false);
  const previous=useRef({x:0,y:0});
  const stateRef=useRef(state);stateRef.current=state;
  const errorRef=useRef(onError);errorRef.current=onError;
  const send=(phase:string,x:number,y:number,shift=false)=>{
    const s=stateRef.current;
    const context=pointerContext.current??{epoch:s.epoch,revision:s.revision,glyph:s.glyph,token:crypto.randomUUID()};
    queue.current=queue.current.then(()=>invoke<void>('pointer',{request:{phase,x,y,shift,...context}})).catch(e=>errorRef.current(referenceError(s.workspace.locale,e)));
  };
  const cancel=()=>{if(!captured.current)return;captured.current=false;cancelAnimationFrame(frame.current);frame.current=0;latest.current=null;send('cancel',0,0);pointerContext.current=null;};
  useEffect(()=>{window.addEventListener('blur',cancel);return()=>{window.removeEventListener('blur',cancel);cancel();};},[]);
  useEffect(()=>{if(dragging)cancel();},[dragging]);
  useEffect(()=>{
    const el=ref.current;if(!el)return;
    let disposed=false;let running=false;let pending=false;
    const sync=async()=>{
      if(disposed)return;if(running){pending=true;return;}running=true;
      const r=el.getBoundingClientRect();
      try{const info=await invoke<string>('canvas',{request:{x:r.x,y:r.y,width:r.width,height:r.height,visible:!dragging}});if(!disposed&&info!=='hidden')setGpu(info);}
      catch(e){if(!disposed){setGpu('');errorRef.current(String(e));}}
      finally{running=false;if(pending){pending=false;void sync();}}
    };
    const observer=new ResizeObserver(()=>void sync());observer.observe(el);
    window.addEventListener('resize',sync);void sync();
    return()=>{disposed=true;observer.disconnect();window.removeEventListener('resize',sync);};
  },[dragging,state.workspace.revision]);
  useEffect(()=>{if(state.proof.active)void proofCommand(state,{type:'prepare',settings:state.proof.settings}).catch(e=>{if(String(e)!=='Conflict')onError(proofError(state,String(e)));});},[state.epoch,state.revision]);
  useEffect(()=>()=>cancelAnimationFrame(frame.current),[]);
  return <section className="editor-center">
    {state.proof.active?<ProofControls state={state} onError={onError}/>:<div className="canvas-tools">
      {(['nodes','pen','pan'] as const).map(tool=><IconButton key={tool} icon={tool} label={t(tool)} active={state.tool===tool} onClick={()=>void edit({type:'tool',tool})}/>)}
      <span className="tool-divider"/>
      <IconButton icon="fill" label={t('fill')} active={state.fill} onClick={()=>void edit({type:'fill'})}/>
      <IconButton icon="nodes" label={t('showNodes')} active={state.nodes} onClick={()=>void edit({type:'nodes'})}/>
      <span className="tool-divider"/>
      <IconButton icon="imageAdd" label={imageText.place} disabled={importing||state.image_editing} onClick={()=>{setImporting(true);void placeReference(state).catch(e=>onError(referenceError(state.workspace.locale,e))).finally(()=>setImporting(false));}}/>
      <IconButton icon="images" label={imageText.select} active={state.tool==='images'} onClick={()=>void edit({type:'tool',tool:'images'})}/>
      <IconButton icon="snippets" label={snippetLabels[state.workspace.locale].title} active={!!state.snippets.placement} onClick={()=>void snippetCommand(state,{type:'open'}).catch(e=>onError(String(e)))}/>
      <span className="tool-spacer"/>
      <IconButton icon="minus" label="−" onClick={()=>void edit({type:'zoom',value:Math.max(.2,state.zoom/1.2)})}/>
      <button className="zoom" onClick={()=>void edit({type:'reset_view'})} title={t('fit')}>{Math.round(state.zoom*100)}%</button>
      <IconButton icon="plus" label="+" onClick={()=>void edit({type:'zoom',value:Math.min(8,state.zoom*1.2)})}/>
      <IconButton icon="fit" label={t('fit')} onClick={()=>void edit({type:'reset_view'})}/>
    </div>}
    {state.snippets.placement&&<div className="snippet-confirm"><Icon name="snippets"/><span>{snippetLabels[state.workspace.locale].preview}</span><IconButton icon="check" label={snippetLabels[state.workspace.locale].commit} onClick={()=>void snippetCommand(state,{type:'commit'}).catch(e=>onError(String(e)))}/><IconButton icon="close" label={snippetLabels[state.workspace.locale].cancel} onClick={()=>void snippetCommand(state,{type:'cancel'}).catch(e=>onError(String(e)))}/></div>}
    <div className={`native-canvas tool-${state.tool}`} ref={ref} role="application" aria-label={t('native')} tabIndex={0}
      onPointerDown={e=>{if(e.button!==0)return;pointerContext.current={epoch:state.epoch,revision:state.revision,glyph:state.glyph,token:crypto.randomUUID()};if(state.proof.active){const r=e.currentTarget.getBoundingClientRect();send('down',e.clientX-r.x,e.clientY-r.y);pointerContext.current=null;return;}e.currentTarget.focus();e.currentTarget.setPointerCapture(e.pointerId);captured.current=true;const r=e.currentTarget.getBoundingClientRect();previous.current={x:e.clientX-r.x,y:e.clientY-r.y};send('down',previous.current.x,previous.current.y,e.shiftKey);}}
      onPointerMove={e=>{if(!captured.current)return;const r=e.currentTarget.getBoundingClientRect();latest.current={x:e.clientX-r.x,y:e.clientY-r.y,shift:e.shiftKey};if(frame.current)return;
        frame.current=requestAnimationFrame(()=>{frame.current=0;const p=latest.current;latest.current=null;if(!p)return;
          if(stateRef.current.tool==='pan'){void edit({type:'pan',dx:p.x-previous.current.x,dy:p.y-previous.current.y});}else{send('move',p.x,p.y,p.shift);}previous.current=p;
        });}}
      onPointerUp={e=>{if(!captured.current)return;captured.current=false;cancelAnimationFrame(frame.current);frame.current=0;const r=e.currentTarget.getBoundingClientRect();const p={x:e.clientX-r.x,y:e.clientY-r.y};
        if(stateRef.current.tool==='nodes'||stateRef.current.tool==='images'||stateRef.current.tool==='snippets')send('move',p.x,p.y,e.shiftKey);send('up',p.x,p.y,e.shiftKey);latest.current=null;pointerContext.current=null;}}
      onPaste={e=>{e.preventDefault();void invoke('paste_paths',{epoch:state.epoch,revision:state.revision,glyph:state.glyph}).catch(e=>onError(referenceError(state.workspace.locale,e)));}}
      onPointerCancel={cancel} onLostPointerCapture={cancel}
      onDoubleClick={()=>{if(state.tool==='pen')void queue.current.then(()=>edit({type:'finish_pen'}));}}
      onKeyDown={e=>{if(e.key==='Enter'&&state.tool==='pen'){e.preventDefault();void edit({type:'finish_pen'});}if(e.key==='Escape'){e.preventDefault();e.stopPropagation();cancel();void queue.current.then(()=>edit({type:'cancel'}));}}}
    >{!gpu&&<span className="canvas-starting">{t('native')}</span>}</div>
    {state.proof.active&&<input className="proof-scroll" aria-label={proofLabels[state.workspace.locale].scroll} type="range" min={0} max={Math.max(0,state.proof.height-(ref.current?.clientHeight||0))} step={20} value={state.proof.settings.scroll} onChange={e=>void proofCommand(state,{type:'scroll',value:Number(e.target.value)}).catch(e=>onError(String(e)))}/>}
    <div className="canvas-caption"><span className="canvas-info" tabIndex={0} role="img" aria-label={state.tool==='snippets'?snippetLabels[state.workspace.locale].help:state.tool==='images'?imageText.help:t(state.tool==='pen'?'penHint':'nodesHint')} title={state.tool==='snippets'?snippetLabels[state.workspace.locale].help:state.tool==='images'?imageText.help:t(state.tool==='pen'?'penHint':'nodesHint')}><Icon name="info"/></span><span title={gpu}>{gpu?t('ready'):t('gpuError')}</span></div>
  </section>;
}
