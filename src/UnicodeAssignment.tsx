import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Icon} from './icons';
import {fontEditMessages} from './font-edit-i18n';
import type {Snapshot} from './types';
export function UnicodeAssignment({state}:{state:Snapshot}){
 const t=fontEditMessages[state.workspace.locale];const [input,setInput]=useState('');const [error,setError]=useState('');const [busy,setBusy]=useState(false);const pending=useRef(false);const token=useRef({epoch:state.epoch,revision:state.revision,id:state.glyph});
 const codes=state.codepoints.join(',');
 useEffect(()=>{setInput(state.codepoints.map(n=>`U+${n.toString(16).toUpperCase().padStart(4,'0')}`).join(' '));setError('');token.current={epoch:state.epoch,revision:state.revision,id:state.glyph};},[state.epoch,state.glyph,codes]);
 useEffect(()=>{token.current.revision=Math.max(token.current.revision,state.revision);},[state.revision]);
 const apply=async()=>{if(pending.current)return;pending.current=true;setBusy(true);setError('');try{const revision=await invoke<number>('edit',{epoch:token.current.epoch,action:{type:'assign_unicode',id:token.current.id,input,revision:token.current.revision}});token.current.revision=Math.max(token.current.revision,revision);}catch(e){setError(String(e)==='UnicodeConflict'?t.conflict:String(e)==='Conflict'?t.mappingStale:t.invalid);}finally{pending.current=false;setBusy(false);}};
 return <details className="unicode-assignment"><summary>{t.unicode}</summary><form onSubmit={e=>{e.preventDefault();void apply();}}><div><span className="panel-help" role="img" tabIndex={0} aria-label={t.mappingHelp} title={t.mappingHelp}><Icon name="info"/></span><input aria-label={t.unicode} placeholder="U+0041 U+FF21" value={input} disabled={busy||state.glyph===0} maxLength={65536} onChange={e=>setInput(e.target.value)}/></div><button type="submit" disabled={busy||state.glyph===0}>{t.assign}</button>{error&&<p role="alert">{error}</p>}</form></details>;
}
