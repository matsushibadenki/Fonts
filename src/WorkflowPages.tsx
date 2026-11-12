import {useMemo,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {proofCommand,proofError} from './ProofControls';
import {Kerning} from './Kerning';
import {Icon} from './icons';
import {messages} from './i18n';
import {fontEditMessages} from './font-edit-i18n';
import {libraryMessages} from './library-i18n';
import {glyphLabel,matchesGlyph} from './unicode';
import type {Edit,Snapshot} from './types';

const labels={
 en:{workflow:'Workflow',select:'Find glyph',current:'Current glyph',characters:'Encoded characters',reviewed:'Reviewed glyphs',outlined:'Glyphs with outlines',check:'Font overview'},
 ja:{workflow:'制作工程',select:'文字を検索',current:'選択中の文字',characters:'Unicode割当数',reviewed:'確認済み字形',outlined:'輪郭のある字形',check:'書体の確認'},
 zh_cn:{workflow:'制作流程',select:'查找字形',current:'当前字形',characters:'已编码字符',reviewed:'已检查字形',outlined:'有轮廓的字形',check:'字体概览'},
};
export const workflowLabels=labels;
export function SpacingPage({state,edit,onError}:{state:Snapshot;edit:(action:Edit)=>Promise<void>;onError:(error:string)=>void}){
 const t=messages[state.workspace.locale],l=labels[state.workspace.locale];
 const [search,setSearch]=useState('');
 const [preparing,setPreparing]=useState(false);
 const options=useMemo(()=>state.glyphs.filter(g=>matchesGlyph(g,search.trim())).slice(0,100),[state.glyphs,search]);
 const current=state.glyphs.find(g=>g.id===state.glyph);
 const choices=current&&!options.some(g=>g.id===current.id)?[current,...options]:options;
 const label=(g:Snapshot['glyphs'][number])=>g.codepoints.length?glyphLabel(g.character):g.name;
 return <aside className="workflow-spacing" aria-label={t.space}>
  <div className="workflow-spacing-heading"><h1>{t.space}</h1><button type="button" disabled={preparing} aria-pressed={state.proof.active&&!state.proof.color} title={t.test} aria-label={t.test} onClick={()=>{setPreparing(true);void proofCommand(state,state.proof.active&&!state.proof.color?{type:'close'}:{type:'prepare',settings:state.proof.settings}).then(()=>onError('')).catch(e=>onError(proofError(state,String(e)))).finally(()=>setPreparing(false));}}><Icon name="eye"/></button></div>
  <label>{l.select}<input type="search" value={search} onChange={e=>setSearch(e.target.value)}/></label>
  <label>{l.current}<select value={state.glyph} onChange={e=>void edit({type:'select_glyph',id:Number(e.target.value)})}>{choices.map(g=><option key={g.id} value={g.id}>{label(g)} · #{g.id}</option>)}</select></label>
  <form key={`${state.epoch}-${state.glyph}-${state.advance}`} onSubmit={e=>{e.preventDefault();const value=Number(new FormData(e.currentTarget).get('advance'));if(value!==state.advance)void edit({type:'advance',id:state.glyph,revision:state.revision,value});}}>
   <label>{t.advance}<input name="advance" type="number" required min={0} max={65535} step="any" defaultValue={state.advance}/></label>
   <button type="submit" title={fontEditMessages[state.workspace.locale].apply} aria-label={fontEditMessages[state.workspace.locale].apply}><Icon name="check"/></button>
  </form>
  <button type="button" className={`review-button ${state.reviewed?'is-reviewed':''}`} onClick={()=>void edit({type:'review',id:state.glyph,revision:state.revision})}><Icon name="check"/>{t[state.reviewed?'reviewed':'review']}</button>
  <Kerning key={`${state.epoch}-${state.glyph}`} state={state} edit={action=>invoke('edit',{epoch:state.epoch,action})}/>
 </aside>;
}
export function FinishPage({state,onProperties,onExport}:{state:Snapshot;onProperties:()=>void;onExport:()=>void}){
 const t=messages[state.workspace.locale],l=labels[state.workspace.locale],m=fontEditMessages[state.workspace.locale];
 const stats=useMemo(()=>state.glyphs.reduce((a,g)=>({encoded:a.encoded+g.codepoints.length,reviewed:a.reviewed+Number(g.reviewed),outlined:a.outlined+Number(g.has_outline)}),{encoded:0,reviewed:0,outlined:0}),[state.glyphs]);
 const metadata=state.properties.metadata;
 return <section className="workflow-finish" aria-label={t.export}>
  <div className="workflow-finish-content">
   <h1>{t.export}</h1><h2>{state.family}</h2>
   <div className="workflow-overview">
    <section aria-label={m.info}><h3>{m.info}</h3><dl>{([[m.style,metadata.style],[m.copyright,metadata.copyright],[m.license,metadata.license],[m.url,metadata.license_url]] as const).map(([key,value])=><div key={key}><dt>{key}</dt><dd>{value||'—'}</dd></div>)}</dl></section>
    <section aria-label={l.check}><h3>{l.check}</h3><dl><div><dt>{l.characters}</dt><dd>{stats.encoded.toLocaleString()}</dd></div><div><dt>{l.reviewed}</dt><dd>{stats.reviewed.toLocaleString()} / {state.glyphs.length.toLocaleString()}</dd></div><div><dt>{l.outlined}</dt><dd>{stats.outlined.toLocaleString()}</dd></div></dl></section>
   </div>
   <div className="workflow-finish-actions"><button onClick={onProperties}><Icon name="info"/>{m.info}</button><button className="primary" onClick={onExport}><Icon name="download"/>{libraryMessages[state.workspace.locale].exportFont}</button></div>
  </div>
 </section>;
}
