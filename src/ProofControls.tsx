import {kerningLabels} from './kerning-i18n';
import {invoke} from '@tauri-apps/api/core';
import {useEffect,useRef,useState} from 'react';
import {IconButton} from './Panel';
import {Icon} from './icons';
import {libraryMessages,libraryError} from './library-i18n';
import type {Snapshot,ProofSettings} from './types';
export const proofLabels={
 en:{orientation:'Text orientation',mixed:'Mixed',upright:'Upright',vertical:'Vertical writing',horizontal:'Horizontal writing',language:'Shaping language',auto:'Automatic',columnHeight:'Column height',text:'Sample text',size:'Font size',tracking:'Tracking (em)',leading:'Line height',pin:'Pin reference',clear:'Remove reference',current:'Current',reference:'Reference',missing:'Missing characters',empty:'Empty glyphs',scroll:'Scroll sample',help:'Shapes the generated TrueType with HarfBuzz, including retained source substitutions, positioning and variation sequences. Supports horizontal and vertical writing. Click a current glyph to edit it; boxes mark missing characters.',failed:'Cannot build proof. Check for open contours or invalid metrics.',stale:'Document changed. Try again.'},
 ja:{orientation:'文字の向き',mixed:'混在',upright:'正立',vertical:'縦組み',horizontal:'横組み',language:'組版の言語',auto:'自動',columnHeight:'列の高さ',text:'試し組みの文字列',size:'文字サイズ',tracking:'字間（em）',leading:'行間',pin:'比較用の見本を固定',clear:'比較を解除',current:'現在',reference:'固定した見本',missing:'未収録文字',empty:'輪郭のない文字',scroll:'試し組みをスクロール',help:'生成したTrueTypeをHarfBuzzで組版し、保持した字形置換・位置調整・異体字シーケンスを反映します。横組みと縦組みに対応します。現在側の文字をクリックすると編集へ戻り、未収録文字は枠で表示します。',failed:'試し組みを生成できません。開いた輪郭や不正なメトリクスを確認してください。',stale:'編集中の文書が変わりました。もう一度お試しください。'},
 zh_cn:{orientation:'文字方向',mixed:'混合',upright:'直立',vertical:'竖排',horizontal:'横排',language:'排版语言',auto:'自动',columnHeight:'列高',text:'示例文本',size:'字号',tracking:'字距（em）',leading:'行高',pin:'固定对照样本',clear:'取消对照',current:'当前',reference:'固定样本',missing:'缺失字符',empty:'空轮廓字形',scroll:'滚动示例',help:'使用HarfBuzz排版生成的TrueType，反映保留的源字形替换、定位和异体序列，支持横排与竖排。点击当前字形可返回编辑，缺失字符以方框表示。',failed:'无法生成预览。请检查开放轮廓或无效度量。',stale:'文档已更改，请重试。'}
};
export function proofError(state:Snapshot,error:string){const key=libraryError(error);return key?libraryMessages[state.workspace.locale][key]:error==='Conflict'?proofLabels[state.workspace.locale].stale:proofLabels[state.workspace.locale].failed;}
export async function proofCommand(state:Snapshot,operation:Record<string,unknown>){await invoke('proof_action',{epoch:state.epoch,revision:state.revision,operation});}
export function ProofControls({state,onError}:{state:Snapshot;onError:(e:string)=>void}){
 const t=proofLabels[state.workspace.locale];const [draft,setDraft]=useState<ProofSettings>(state.proof.settings);const latest=useRef(state);latest.current=state;const first=useRef(true);
 useEffect(()=>{if(first.current){first.current=false;return;}const timer=setTimeout(()=>{void proofCommand(latest.current,{type:'prepare',settings:draft}).then(()=>onError('')).catch(e=>{if(String(e)!=='Conflict')onError(proofError(latest.current,String(e)));});},350);return()=>clearTimeout(timer);},[draft,onError,t]);
 useEffect(()=>{setDraft(previous=>JSON.stringify(previous)===JSON.stringify(state.proof.settings)?previous:state.proof.settings);},[state.proof.settings.orientation,state.proof.settings.direction,state.proof.settings.language,state.proof.settings.column_height,state.proof.settings.text,state.proof.settings.kerning,state.proof.settings.size,state.proof.settings.tracking,state.proof.settings.leading]);
 const act=(operation:Record<string,unknown>)=>void proofCommand(state,operation).catch(e=>onError(String(e)));
 return <>
  <div className="canvas-tools proof-tools">
   {(['size','tracking','leading'] as const).map(k=><label key={k} title={t[k]}><span>{k==='size'?'Aa':k==='tracking'?'↔':'↕'}</span><input aria-label={t[k]} type="number" value={draft[k]} min={k==='size'?12:k==='tracking'?-.1:.8} max={k==='size'?160:k==='tracking'?1:3} step={k==='size'?1:.05} onChange={e=>{const value=Number(e.target.value);if(e.target.value&&Number.isFinite(value))setDraft({...draft,[k]:value,scroll:0});}}/></label>)}
   <IconButton icon="verticalText" label={draft.direction==='vertical'?t.horizontal:t.vertical} active={draft.direction==='vertical'} onClick={()=>setDraft({...draft,direction:draft.direction==='vertical'?'horizontal':'vertical',scroll:0})}/>
   <select aria-label={t.language} value={draft.language??'und'} onChange={e=>setDraft({...draft,language:e.target.value,scroll:0})}><option value="und">{t.auto}</option><option value="en">English</option><option value="ja">日本語</option><option value="zh-Hans">简体中文</option></select>
   {draft.direction==='vertical'&&<select aria-label={t.orientation} value={draft.orientation??'mixed'} onChange={e=>setDraft({...draft,orientation:e.target.value as ProofSettings['orientation'],scroll:0})}><option value="mixed">{t.mixed}</option><option value="upright">{t.upright}</option></select>}
   {draft.direction==='vertical'&&<label title={t.columnHeight}><span>↧</span><input aria-label={t.columnHeight} type="number" min={120} max={2400} step={20} value={draft.column_height??600} onChange={e=>{const n=Number(e.target.value);if(n>=120&&n<=2400)setDraft({...draft,column_height:n,scroll:0});}}/></label>}
   <IconButton icon="kerning" label={kerningLabels[state.workspace.locale].enabled} active={draft.kerning} onClick={()=>setDraft({...draft,kerning:!draft.kerning,scroll:0})}/>
   <span className="tool-spacer"/><IconButton icon="save" label={t.pin} onClick={()=>act({type:'pin'})}/>{state.proof.comparison&&<IconButton icon="close" label={t.clear} onClick={()=>act({type:'clear_reference'})}/>}
   <span className="panel-help" role="img" tabIndex={0} aria-label={t.help} title={t.help}><Icon name="info"/></span>
  </div>
  <textarea className="proof-text" aria-label={t.text} maxLength={4096} value={draft.text} onChange={e=>setDraft({...draft,text:e.target.value,scroll:0})}/>
  <div className="proof-summary">{state.proof.source&&<span title={state.proof.source}>{state.proof.source}</span>}<span title={state.proof.missing.map(c=>`U+${c.toString(16).toUpperCase()}`).join(', ')}>{t.missing}: {state.proof.missing.length} · {t.empty}: {state.proof.empty}</span>{state.proof.comparison&&<span>{t.current} | {t.reference}</span>}</div>
 </>;
}
