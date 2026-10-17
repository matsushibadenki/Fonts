import {invoke,isTauri} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import {useCallback,useEffect,useRef,useState} from 'react';
import type {Edit,Layout,Snapshot} from './types';
export function useWorkspace(){
  const [state,setState]=useState<Snapshot|null>(null);
  const [error,setError]=useState('');
  const generation=useRef(0);
  const current=useRef<Snapshot|null>(null);
  const refresh=useCallback(async()=>{
    const n=++generation.current;
    try{const s=await invoke<Snapshot>('snapshot');if(n===generation.current){current.current=s;setState(s);}}
    catch(e){setError(String(e));}
  },[]);
  useEffect(()=>{
    if(!isTauri()){setError('native_required');return;}
    let disposed=false;let unlisten:undefined|(()=>void);let unerror:undefined|(()=>void);let unmenu:undefined|(()=>void);
    void Promise.all([listen('state-changed',()=>void refresh()),listen<string>('canvas-error',e=>setError(e.payload)),listen<string>('menu-edit',e=>{
      const target=document.activeElement;
      if(target instanceof HTMLInputElement||target instanceof HTMLTextAreaElement||(target instanceof HTMLElement&&target.isContentEditable)){
        document.execCommand(e.payload==='redo'?'redo':'undo');return;
      }
      const s=current.current;if(!s||(e.payload==='undo'?!s.can_undo:!s.can_redo))return;
      void invoke('edit',{epoch:s.epoch,action:{type:e.payload,revision:s.revision}}).catch(e=>setError(String(e)));
    })]).then(([a,b,c])=>{if(disposed){a();b();c();}else{unlisten=a;unerror=b;unmenu=c;void refresh();}}).catch(e=>setError(String(e)));
    const focus=()=>{const el=document.activeElement;void invoke('text_focus',{focused:el instanceof HTMLInputElement||el instanceof HTMLTextAreaElement||(el instanceof HTMLElement&&el.isContentEditable)}).catch(()=>{});};
    const scheduleFocus=()=>queueMicrotask(focus);
    document.addEventListener('focusin',scheduleFocus);document.addEventListener('focusout',scheduleFocus);
    return()=>{document.removeEventListener('focusin',scheduleFocus);document.removeEventListener('focusout',scheduleFocus);disposed=true;unlisten?.();unerror?.();unmenu?.();generation.current++;};
  },[refresh]);
  const run=useCallback(async(command:string,args?:Record<string,unknown>)=>{
    try{await invoke(command,args);setError('');}catch(e){setError(String(e));void refresh();}
  },[refresh]);
  const edit=useCallback((action:Edit)=>run('edit',{action,epoch:current.current?.epoch}),[run]);
  const layout=useCallback((action:Layout)=>run('layout',{action,revision:state?.workspace.revision}),[run,state?.workspace.revision]);
  return{state,error,setError,edit,layout,run,refresh};
}
