import {PanelArrangement} from './PanelArrangement';
import {invoke} from '@tauri-apps/api/core';
import {Kerning} from './Kerning';
import {Snippets} from './Snippets';
import {snippetCommand,snippetError,snippetLabels} from './snippet-i18n';
import {ReferenceImages} from './ReferenceImages';
import {GlyphThumbnail} from './GlyphThumbnail';
import {ContourEditor} from './ContourEditor';
import {UnicodeAssignment} from './UnicodeAssignment';
import {fontEditMessages} from './font-edit-i18n';
import type {FontEditKey} from './font-edit-i18n';
import {useDeferredValue,useMemo,useRef,useState} from 'react';
import {glyphLabel,matchesGlyph,unicodeCode} from './unicode';
import {Icon} from './icons';
import type {IconName} from './icons';
import type {Edit,Layout,Panel as PanelId,Snapshot} from './types';
import type {Key} from './i18n';
type T=(key:Key)=>string;
export function IconButton({icon,label,onClick,active=false,disabled=false}:{icon:IconName;label:string;onClick:()=>void;active?:boolean;disabled?:boolean}){
  return <button type="button" className={`icon-button ${active?'selected':''}`} title={label} aria-label={label} aria-pressed={active} onClick={onClick} disabled={disabled}><Icon name={icon}/></button>;
}
export function Panel({id,state,t,edit,layout,floating=false,onDrag}:{id:PanelId;state:Snapshot;t:T;edit:(a:Edit)=>Promise<void>;layout:(a:Layout)=>Promise<void>;floating?:boolean;onDrag?:(p:PanelId|null)=>void}){
  const [arranging,setArranging]=useState(false);
  const [snippetFailure,setSnippetFailure]=useState('');
  const [search,setSearch]=useState('');
  const [adding,setAdding]=useState(false);
  const [input,setInput]=useState('');
  const [codepoints,setCodepoints]=useState(false);
  const [page,setPage]=useState(0);
  const [preset,setPreset]=useState('ascii');
  const ft=fontEditMessages[state.workspace.locale];
  const deferredSearch=useDeferredValue(search);
  const filtered=useMemo(()=>state.glyphs.filter(g=>matchesGlyph(g,deferredSearch)),[state.glyphs,deferredSearch]);
  const pages=Math.max(1,Math.ceil(filtered.length/256));
  const currentPage=Math.min(page,pages-1);

  const dragStart=useRef<{x:number;y:number}|null>(null);
  const started=useRef(false);
  return <section className={`panel panel-${id}`} aria-label={t(id)}>
    <header className="panel-header" onPointerDown={e=>{if(floating||e.button!==0||(e.target as Element).closest('button,.panel-help'))return;e.preventDefault();dragStart.current={x:e.clientX,y:e.clientY};started.current=false;e.currentTarget.setPointerCapture(e.pointerId);}}
      onPointerMove={e=>{const p=dragStart.current;if(p&&!started.current&&Math.hypot(e.clientX-p.x,e.clientY-p.y)>6){started.current=true;onDrag?.(id);}}}
      onPointerUp={()=>{dragStart.current=null;started.current=false;}}
      onPointerCancel={()=>{dragStart.current=null;started.current=false;onDrag?.(null);}}>
      <span className="grip"><Icon name="grip"/></span><strong>{t(id)}</strong>
      <span className="panel-help" tabIndex={0} role="img" aria-label={t(id==='glyphs'?'glyphHelp':id==='metrics'?'metricHelp':id==='snippets'?'snippetHelp':'outlineHelp')} title={t(id==='glyphs'?'glyphHelp':id==='metrics'?'metricHelp':id==='snippets'?'snippetHelp':'outlineHelp')}><Icon name="info"/></span>
      <IconButton icon="split" label={t('panelLayout')} active={arranging} onClick={()=>setArranging(!arranging)}/>
      {!floating && <IconButton icon="popout" label={t('popout')} onClick={()=>void layout({type:'float',panel:id})}/>}
    </header>
    {arranging&&<PanelArrangement id={id} state={state} t={t} layout={layout}/>}
    {floating && <div className="float-docks" title={t('floatHelp')}>{(['left','right','bottom'] as const).map(d=><IconButton key={d} icon={d} label={t(d)} onClick={()=>void layout({type:'dock',panel:id,dock:d})}/>)}</div>}
    {id==='glyphs'&&<div className="glyph-controls">
        <div className="glyph-search"><label className="search"><Icon name="search"/><input placeholder={t('search')} aria-label={t('search')} value={search} onChange={e=>{setSearch(e.target.value);setPage(0);}}/></label><IconButton icon="plus" label={t('addCharacters')} active={adding} onClick={()=>setAdding(!adding)}/></div>
        {adding&&<form className="unicode-add" onSubmit={e=>{e.preventDefault();void edit({type:'add_characters',input,codepoints,revision:state.revision});}}>
          <div className="preset-row"><select aria-label={ft.presets} value={preset} onChange={e=>setPreset(e.target.value)}>{state.presets.map(([id,count])=><option key={id} value={id}>{ft[id as FontEditKey]} ({count})</option>)}</select><IconButton icon="plus" label={ft.addPreset} onClick={()=>void edit({type:'add_preset',preset,revision:state.revision})}/><span className="panel-help" role="img" tabIndex={0} aria-label={ft.presetHelp} title={ft.presetHelp}><Icon name="info"/></span></div>
          <div className="unicode-mode"><select aria-label={t('inputMode')} value={codepoints?'codes':'text'} onChange={e=>setCodepoints(e.target.value==='codes')}><option value="text">{t('characters')}</option><option value="codes">{t('codepoints')}</option></select><span className="panel-help" tabIndex={0} title={t('unicodeHelp')} aria-label={t('unicodeHelp')}><Icon name="info"/></span></div>
          <input aria-label={t('addCharacters')} placeholder={codepoints?'U+3041-U+3096, U+1F600':'あいう 中文 😀'} value={input} onChange={e=>setInput(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&e.nativeEvent.isComposing)e.preventDefault();}} maxLength={262136}/>
          <button type="submit" className="review-button" disabled={!input}><Icon name="plus"/>{t('addCharacters')}</button>
        </form>}
    </div>}
    <div className="panel-body">
      {snippetFailure&&<div className="reference-error" role="alert">{snippetFailure}</div>}
      {id==='glyphs' && <>
        <div className="glyph-grid">{filtered.slice(currentPage*256,(currentPage+1)*256).map(g=><button key={g.id} className={`glyph ${g.id===state.glyph?'current':''}`} onClick={()=>void edit({type:'select_glyph',id:g.id})} title={`${g.name}${g.cid!==null?` · CID ${g.cid}`:''} · ${g.codepoints.map(unicodeCode).join(', ')}`} aria-label={`${g.name}${g.cid!==null?` CID ${g.cid}`:''} ${glyphLabel(g.character)} ${g.codepoints.map(unicodeCode).join(', ')}`} aria-pressed={g.id===state.glyph}>
          <GlyphThumbnail key={`${state.epoch}-${state.revision}-${g.id}`} glyph={g} epoch={state.epoch} revision={state.revision} locale={state.workspace.locale}/><small>{g.codepoints.length?g.codepoints[0].toString(16).toUpperCase().padStart(4,'0'):g.cid!==null?`CID${g.cid}`:'—'}</small><i className={g.reviewed?'reviewed':g.has_outline?'drawn':''}/>
        </button>)}</div>
        {!filtered.length&&<p className="empty">{t('noMatches')}</p>}


      </>}
      {id==='snippets'&&<Snippets state={state}/>}
      {id==='metrics' && <div className="metrics-form">
        <label className="metric"><span>{t('advance')}</span><input key={`${state.glyph}-${state.advance}`} type="number" min="0" max="65535" step="1" aria-label={t('advance')} defaultValue={state.advance} onBlur={e=>{if(e.target.value!==''&&Number(e.target.value)!==state.advance)void edit({type:'advance',value:Number(e.target.value),revision:state.revision,id:state.glyph});}} onKeyDown={e=>{if(e.key==='Enter')e.currentTarget.blur();}}/></label>
        <Kerning key={`kern-${state.epoch}-${state.glyph}`} state={state} edit={async action=>{await invoke('edit',{epoch:state.epoch,action});}}/>
        <UnicodeAssignment key={`${state.epoch}-${state.glyph}`} state={state}/>
        <div className="metric-divider"/>
        {(['ascender','descender','upm'] as const).map(k=><label className="metric" key={k}><span>{t(k)}</span><input readOnly aria-label={t(k)} value={state[k]}/></label>)}
        <button className={`review-button ${state.reviewed?'is-reviewed':''}`} onClick={()=>void edit({type:'review',id:state.glyph,revision:state.revision})}><Icon name="check"/>{t(state.reviewed?'reviewed':'review')}</button>
      </div>}
      {id==='outline' && <div className="outline-sections"><ReferenceImages key={`${state.epoch}-${state.glyph}`} state={state}/><div>
        <div className="panel-count">{state.contours.length} {t('contours')}</div>
        {state.contours.length===0?<p className="empty">{t('empty')}</p>:<div className="contour-list">{state.contours.map((c,i)=><div className="contour-item" key={i}><div className="contour-row"><input type="checkbox" aria-label={`${snippetLabels[state.workspace.locale].selectContour} #${i+1}`} checked={state.snippets.contours.includes(i)} disabled={!!state.snippets.placement} onChange={e=>void snippetCommand(state,{type:'contour',index:i,selected:e.target.checked}).then(()=>setSnippetFailure('')).catch(e=>setSnippetFailure(snippetError(state.workspace.locale,e)))}/><Icon name="contour"/><span>#{i+1}</span><span>{c.nodes} {t('points')}</span><span className="contour-state" title={t(c.closed?'closed':'open')}>{c.closed?'○':'⌒'}</span></div><ContourEditor key={`${state.epoch}-${state.glyph}-${i}`} state={state} index={i} edit={edit}/></div>)}</div>}
      </div></div>}
    </div>
    {id==='glyphs'&&<footer className="panel-footer glyph-footer">{pages>1&&<div className="glyph-pages"><button disabled={currentPage===0} onClick={()=>setPage(currentPage-1)} aria-label={t('previousPage')}>‹</button><input className="glyph-page-input" type="number" min={1} max={pages} aria-label={t('pageNumber')} value={currentPage+1} onChange={e=>{const n=Number(e.target.value);if(Number.isInteger(n)&&n>=1&&n<=pages)setPage(n-1);}}/><span>/ {pages}</span><button disabled={currentPage===pages-1} onClick={()=>setPage(currentPage+1)} aria-label={t('nextPage')}>›</button></div>}<span>{state.glyphs.filter(g=>g.reviewed).length} / {state.glyphs.length}</span><span>{t('reviewed')}</span></footer>}
  </section>;
}
