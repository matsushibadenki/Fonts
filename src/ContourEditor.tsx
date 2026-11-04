import {useEffect,useRef,useState} from 'react';
import {pathLabels,processingLabels} from './path-i18n';
import {Icon} from './icons';
import type {Edit,Snapshot} from './types';
const messages={
  en:{point:'Point',start:'Start',split:'Add midpoint',remove:'Delete point',smooth:'Smooth',corner:'Corner',help:'Select a canvas point or choose its coordinates here. Handles select their attached anchor. + splits the incoming segment (the first segment for Start); − deletes the anchor. Smooth aligns both handles and couples their dragging; Corner releases coupling without moving handles. Adjacent lines/quadratics become cubic curves when smoothing. Open endpoints cannot be smoothed.'},
  ja:{point:'点',start:'始点',split:'中間点を追加',remove:'点を削除',smooth:'滑らか',corner:'角',help:'キャンバスの点、または座標一覧から選びます。ハンドルの選択は接続先の端点と連動します。＋は手前の区間（始点では最初の区間）を二分割、−は端点を削除します。滑らかは両ハンドルを一直線に揃え、ドラッグを連動させます。角は形を保って連動を解除します。滑らかへの切替時は隣接する直線・二次曲線を三次曲線に変換します。開輪郭の両端は滑らかにできません。'},
  zh_cn:{point:'点',start:'起点',split:'添加中点',remove:'删除点',smooth:'平滑',corner:'尖角',help:'在画布或坐标列表中选择点。控制柄对应其连接的锚点。＋拆分前一线段（起点对应第一段）；−删除锚点。平滑对齐两侧控制柄并联动拖动；尖角保持形状并解除联动。平滑时相邻直线和二次曲线转换为三次曲线。开放轮廓的端点不能设为平滑。'},
};
export function ContourEditor({state,index,edit}:{state:Snapshot;index:number;edit:(a:Edit)=>Promise<void>}){
  const [pending,setPending]=useState(false);
  const [target,setTarget]=useState('');
  const ref=useRef<HTMLDivElement>(null);
  const c=state.contours[index];
  const processing=processingLabels[state.workspace.locale];
  const paths=pathLabels[state.workspace.locale];
  const t=messages[state.workspace.locale];
  const active=state.selection?.contour===index;
  const anchor=active?state.selection!.anchor:Math.min(1,c.segments.length);
  const segment=Math.max(0,anchor-1);
  const endpoint=!c.closed&&c.segments.length>0&&(anchor===0||anchor===c.segments.length);
  const targets=!active||!endpoint?[]:state.contours.flatMap((other,i)=>other.closed||!other.segments.length?[]:[false,true].filter(end=>i!==index||end!==(anchor!==0)).map(end=>({value:`${i}:${end?'end':'start'}`,label:`#${i+1} · ${end?paths.end:paths.start}`}))).sort((a,b)=>Number(a.value.startsWith(`${index}:`))-Number(b.value.startsWith(`${index}:`)));
  const chosen=targets.some(t=>t.value===target)?target:targets[0]?.value??'';

  useEffect(()=>{if(active)ref.current?.scrollIntoView({block:'nearest',inline:'nearest'});},[active,anchor]);
  const apply=async(type:string,extra:Record<string,unknown>={})=>{
    if(pending)return;
    setPending(true);
    try{await edit({type,id:state.glyph,contour:index,anchor,segment,revision:state.revision,...extra});}finally{setPending(false);}
  };
  return <div ref={ref} className={`contour-edit ${active?'active-node':''}`}>
    <label title={t.point}><select aria-label={`${t.point} #${index+1}`} value={anchor} onFocus={()=>{if(!active)void edit({type:'select_node',id:state.glyph,contour:index,anchor,revision:state.revision});}} onChange={e=>void apply('select_node',{anchor:Number(e.target.value)})} disabled={pending}>
      <option value={0}>{t.start} · {c.start.map(v=>Math.round(v*100)/100).join(', ')}</option>
      {c.segments.map((s,i)=>s.closure?null:<option key={i} value={i+1}>#{i+1} · {s.end.map(v=>Math.round(v*100)/100).join(', ')}</option>)}
    </select></label>
    <button className="icon-button" type="button" title={t.split} aria-label={`${t.split} #${index+1}`} disabled={pending||!c.segments.length||c.segments.length>=65536} onClick={()=>void apply('split_segment')}><Icon name="plus"/></button>
    <button className="icon-button" type="button" title={t.remove} aria-label={`${t.remove} #${index+1}`} disabled={pending||anchor===0||!c.segments[segment]?.deletable} onClick={()=>void apply('delete_segment_end')}><span aria-hidden="true">−</span></button>
    <button className={`icon-button ${active&&state.selection?.smooth?'selected':''}`} aria-pressed={active&&state.selection?.smooth===true} aria-label={`${t.smooth} #${index+1}`} title={t.smooth} disabled={pending||!c.segments.length||(!c.closed&&(anchor===0||anchor===c.segments.length))} onClick={()=>void apply('node_mode',{smooth:true})}><span aria-hidden="true">◡</span></button>
    <button className={`icon-button ${active&&!state.selection?.smooth?'selected':''}`} aria-pressed={active&&state.selection?.smooth===false} aria-label={`${t.corner} #${index+1}`} title={t.corner} disabled={pending} onClick={()=>void apply('node_mode',{smooth:false})}><span aria-hidden="true">⌃</span></button>
    <span className="panel-help" tabIndex={0} role="img" aria-label={t.help} title={t.help}><Icon name="info"/></span>
    <div className="contour-path-actions">
    <button className="icon-button" type="button" title={processing.split} aria-label={`${processing.split} #${index+1}`} disabled={pending||!active||!!state.pen_points||!c.segments.length||(!c.closed&&(anchor===0||anchor===c.segments.length))} onClick={()=>void apply('split_path')}><Icon name="cutPath"/></button>
    <button className="icon-button" type="button" title={paths.reverse} aria-label={`${paths.reverse} #${index+1}`} disabled={pending||!c.segments.length} onClick={()=>void apply('path_operation',{operation:'reverse'})}><Icon name="reversePath"/></button>
    <button className="icon-button" type="button" title={paths.toggle} aria-label={`${paths.toggle} #${index+1}`} disabled={pending||!c.segments.length} onClick={()=>void apply('path_operation',{operation:'toggle_closed'})}><Icon name={c.closed?'openPath':'closePath'}/></button>
    <button className="icon-button" type="button" title={paths.straighten} aria-label={`${paths.straighten} #${index+1}`} disabled={pending||!c.segments.length} onClick={()=>void apply('path_operation',{operation:'straighten'})}><Icon name="straightPath"/></button>
      <span className="panel-help" tabIndex={0} role="img" aria-label={paths.help} title={paths.help}><Icon name="info"/></span>
    </div>
    <div className="contour-endpoint-actions">
      <button className="icon-button" type="button" title={paths.continue} aria-label={`${paths.continue} #${index+1}`} disabled={pending||!endpoint||!!state.pen_points} onClick={()=>void apply('continue_path')}><Icon name="continuePath"/></button>
      <label title={paths.target}><select aria-label={`${paths.target} #${index+1}`} value={chosen} disabled={pending||!active||!endpoint||!!state.pen_points} onChange={e=>setTarget(e.target.value)}>{targets.map(t=><option key={t.value} value={t.value}>{t.label}</option>)}</select></label>
      <button className="icon-button" type="button" title={paths.join} aria-label={`${paths.join} #${index+1}`} disabled={pending||!endpoint||!chosen||!!state.pen_points} onClick={()=>void apply('join_paths',{target:Number(chosen.split(':')[0]),target_end:chosen.endsWith(':end')})}><Icon name="joinPath"/></button>
      <button className="icon-button" type="button" title={paths.independent} aria-label={`${paths.independent} #${index+1}`} disabled={pending||!active||!state.selection?.smooth||!!state.pen_points} onClick={()=>void apply('node_mode',{smooth:false})}><Icon name="independent"/></button>
      <span className="panel-help" tabIndex={0} role="img" aria-label={paths.endpointHelp} title={paths.endpointHelp}><Icon name="info"/></span>
    </div>
  </div>;
}
