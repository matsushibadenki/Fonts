import {invoke} from '@tauri-apps/api/core';
import {useEffect,useRef,useState} from 'react';
import {IconButton} from './Panel';
import {Icon} from './icons';
import {referenceLabels,referenceError} from './reference-i18n';
import type {ReferenceImage,Snapshot} from './types';
export function referenceCommand(state:Snapshot,action:Record<string,unknown>){return invoke<void>('reference_action',{epoch:state.epoch,revision:state.revision,glyph:state.glyph,action});}
export function placeReference(state:Snapshot){return invoke<void>('reference_import',{epoch:state.epoch,revision:state.revision,glyph:state.glyph});}
export function ReferenceImages({state}:{state:Snapshot}){
  const t=referenceLabels[state.workspace.locale];
  const [error,setError]=useState('');const [busy,setBusy]=useState(false);
  const queue=useRef(Promise.resolve());const latest=useRef(state);latest.current=state;
  const gesture=useRef<{token:string;state:Snapshot}|null>(null);
  const range=useRef<HTMLInputElement>(null);
  const selected=state.images.find(i=>i.id===state.image_selected);
  const index=state.images.findIndex(i=>i.id===state.image_selected);
  const run=(action:Record<string,unknown>,context=latest.current)=>{
    queue.current=queue.current.then(()=>referenceCommand(context,action)).then(()=>setError('')).catch(e=>setError(referenceError(context.workspace.locale,e)));
  };
  const end=(cancel=false)=>{const g=gesture.current;if(!g)return;gesture.current=null;run({type:cancel?'cancel':'opacity_end',token:g.token},g.state);};
  useEffect(()=>{if(range.current&&!gesture.current)range.current.value=String(Math.round((selected?.opacity??.5)*100));},[selected?.opacity,state.image_editing,state.revision,state.image_selected]);
  useEffect(()=>{const blur=()=>end(true);window.addEventListener('blur',blur);return()=>{window.removeEventListener('blur',blur);const g=gesture.current;if(g){gesture.current=null;queue.current=queue.current.then(()=>referenceCommand(g.state,{type:'cancel',token:g.token})).catch(()=>{});}};},[]);
  const update=(image:ReferenceImage)=>run({type:'update',image});
  const moveOpacity=(input:HTMLInputElement,x:number)=>{const g=gesture.current;if(!g)return;const rect=input.getBoundingClientRect();const value=Math.round(Math.max(0,Math.min(100,(x-rect.left-8)/Math.max(1,rect.width-16)*100)));input.value=String(value);run({type:'opacity_move',token:g.token,value:value/100},g.state);};
  return <section className="reference-panel" aria-label={t.images}>
    <div className="reference-heading"><Icon name="image"/><strong>{t.images}</strong><span className="reference-count">{state.images.length}</span><span className="tool-spacer"/><span className="panel-help" role="img" tabIndex={0} title={t.help} aria-label={t.help}><Icon name="info"/></span><IconButton icon="imageAdd" label={t.place} disabled={busy||state.image_editing||state.proof.active} onClick={()=>{setBusy(true);void placeReference(state).catch(e=>setError(referenceError(state.workspace.locale,e))).finally(()=>setBusy(false));}}/></div>
    {state.images.length?<>
      <div className="reference-list" role="listbox" aria-label={t.images}>{state.images.map(image=><button key={image.id} role="option" aria-selected={image.id===state.image_selected} className={image.id===state.image_selected?'active':''} disabled={state.image_editing} onClick={()=>run({type:'select',id:image.id})} title={`#${image.id} · ${Math.round(image.width)} × ${Math.round(image.height)}`}><Icon name={image.locked?'lock':image.visible?'image':'eyeOff'}/><span>#{image.id}</span></button>)}</div>
      {selected&&<><div className="reference-actions">
        <IconButton icon={selected.visible?'eye':'eyeOff'} label={selected.visible?t.hide:t.show} active={!selected.visible} disabled={state.image_editing} onClick={()=>update({...selected,visible:!selected.visible})}/>
        <IconButton icon={selected.locked?'lock':'unlock'} label={selected.locked?t.unlock:t.lock} active={selected.locked} disabled={state.image_editing} onClick={()=>update({...selected,locked:!selected.locked})}/>
        <span className="tool-divider"/>
        <IconButton icon="layerDown" label={t.back} disabled={state.image_editing||index<=0} onClick={()=>run({type:'reorder',id:selected.id,index:index-1})}/>
        <IconButton icon="layerUp" label={t.front} disabled={state.image_editing||index===state.images.length-1} onClick={()=>run({type:'reorder',id:selected.id,index:index+1})}/>
        <span className="tool-spacer"/><IconButton icon="trash" label={t.remove} disabled={state.image_editing} onClick={()=>run({type:'remove',id:selected.id})}/>
      </div>
      <div className="reference-fields">{(['x','y','width','height','angle','opacity'] as const).map(field=>{
        const value=field==='x'?selected.center[0]:field==='y'?selected.center[1]:field==='opacity'?selected.opacity*100:selected[field];
        return <label key={field}><span>{t[field]}</span><input key={`${state.epoch}-${selected.id}-${value}`} aria-label={`${t.images} ${t[field]}`} type="number" step={field==='opacity'?1:'any'} min={field==='width'||field==='height'?.001:field==='opacity'?0:field==='angle'?-360:-1000000} max={field==='opacity'?100:field==='angle'?360:1000000} disabled={state.image_editing||(selected.locked&&field!=='opacity')} defaultValue={Number(value.toFixed(3))} onBlur={e=>{const n=Number(e.target.value);if(!e.target.value||!Number.isFinite(n)||!e.target.validity.valid){e.target.value=String(Number(value.toFixed(3)));setError(t.InvalidGeometry);return;}if(Math.abs(n-value)<.0005)return;e.target.value=String(Number(value.toFixed(3)));const image={...selected};if(field==='x'||field==='y')image.center=field==='x'?[n,selected.center[1]]:[selected.center[0],n];else if(field==='opacity')image.opacity=n/100;else image[field]=n;update(image);}} onKeyDown={e=>{if(e.key==='Enter')e.currentTarget.blur();if(e.key==='Escape'){e.currentTarget.value=String(value);e.currentTarget.blur();}}}/></label>;
      })}</div>
      <input ref={range} className="reference-opacity" type="range" min={0} max={100} step={1} defaultValue={selected.opacity*100} aria-label={t.opacity} disabled={state.image_editing&&!gesture.current} onPointerDown={e=>{if(e.button!==0)return;e.preventDefault();e.currentTarget.focus();const token=crypto.randomUUID();gesture.current={token,state:latest.current};e.currentTarget.setPointerCapture(e.pointerId);run({type:'opacity_begin',id:selected.id,token});moveOpacity(e.currentTarget,e.clientX);}} onPointerMove={e=>moveOpacity(e.currentTarget,e.clientX)} onChange={e=>{const g=gesture.current;if(g)run({type:'opacity_move',token:g.token,value:Number(e.target.value)/100},g.state);else update({...selected,opacity:Number(e.target.value)/100});}} onPointerUp={e=>{moveOpacity(e.currentTarget,e.clientX);end();}} onPointerCancel={()=>end(true)} onLostPointerCapture={()=>end(true)} onKeyDown={e=>{if(e.key==='Escape'){e.stopPropagation();end(true);}}}/>
      </>}
    </>:<span className="reference-empty">{t.empty}</span>}
    {error&&<div className="reference-error" role="alert">{error}</div>}
  </section>;
}
