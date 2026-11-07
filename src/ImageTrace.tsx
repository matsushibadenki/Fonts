import {useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {IconButton} from './Panel';
import {Icon} from './icons';
import type {Snapshot} from './types';
const labels={
 en:{title:'Image trace',threshold:'Threshold',invert:'Invert',area:'Min. area',tolerance:'Simplify',preview:'Preview trace',apply:'Add outlines',cancel:'Cancel trace',help:'Monochrome outlines from the selected PNG/JPEG. Alpha is composited onto white; fully transparent pixels stay empty. Area and simplification use source pixels. Preview uses the placed image transform. Apply adds editable closed paths in one undo step; the image stays. Up to 4 million pixels, 500,000 boundary edges and 65,536 points. Variable source documents require a static copy.',trace_empty:'No outlines at these settings.',trace_too_large:'Trace limit exceeded. Use a smaller or cleaner image.',trace_unavailable:'Finish the current edit; variable fonts require a static copy.',error:'Unable to trace. Check the selected image and document.'},
 ja:{title:'画像トレース',threshold:'しきい値',invert:'反転',area:'最小面積',tolerance:'簡略化',preview:'トレースをプレビュー',apply:'輪郭を追加',cancel:'トレースを取り消す',help:'選択したPNG/JPEGを白黒の輪郭に変換します。透明度は白地に合成し、完全に透明な画素は空白のままにします。面積と簡略化は元画像の画素単位です。配置した位置・拡縮・回転でプレビューし、確定すると編集可能な閉じたパスを追加します。元画像は残り、1回のUndoで戻せます。400万画素・境界50万辺・65536点まで。可変フォントは静的コピーで利用できます。',trace_empty:'この設定では輪郭がありません。',trace_too_large:'トレースの上限を超えました。画像を縮小するかノイズを減らしてください。',trace_unavailable:'現在の編集を終了してください。可変フォントは静的コピーで利用できます。',error:'トレースできません。選択画像と文書を確認してください。'},
 zh_cn:{title:'图像描摹',threshold:'阈值',invert:'反相',area:'最小面积',tolerance:'简化',preview:'预览描摹',apply:'添加轮廓',cancel:'取消描摹',help:'将所选 PNG/JPEG 转为黑白轮廓。透明度与白色背景合成，完全透明的像素保持空白。面积和简化以原图像素为单位。预览保留放置位置、缩放和旋转；确认后添加可编辑闭合路径，可一次撤销。保留原图。上限为400万像素、50万边、65536点。可变字体请先创建静态副本。',trace_empty:'当前设置没有轮廓。',trace_too_large:'超出描摹上限。请缩小图像或减少噪点。',trace_unavailable:'请完成当前编辑；可变字体需要静态副本。',error:'无法描摹，请检查所选图像和文档。'}
};
export function ImageTrace({state,id}:{state:Snapshot;id:number}){
 const t=labels[state.workspace.locale];
 const [settings,setSettings]=useState({threshold:128,invert:false,min_area:4,tolerance:.5});
 const [busy,setBusy]=useState(false);const [dirty,setDirty]=useState(true);const [error,setError]=useState('');
 const run=(action:Record<string,unknown>)=>{setBusy(true);setError('');void invoke('reference_trace',{epoch:state.epoch,revision:state.revision,glyph:state.glyph,action}).then(()=>setDirty(action.type!=='preview')).catch(e=>{const key=String(e) as keyof typeof t;setError(t[key]||t.error);}).finally(()=>setBusy(false));};
 const disabled=busy||state.proof.active||!!state.source_font||!!state.master_edit;
 return <details className="image-trace" aria-busy={busy}><summary>{t.title}<span className="panel-help" tabIndex={0} title={t.help} aria-label={t.help}><Icon name="info"/></span></summary>
 <div className="reference-fields">{(['threshold','min_area','tolerance'] as const).map((key,i)=><label key={key}><span>{[t.threshold,t.area,t.tolerance][i]}</span><input aria-label={`${t.title} ${[t.threshold,t.area,t.tolerance][i]}`} type="number" min={0} max={i===0?255:i===1?1000000:4} step={i===0?1:i===1?1:.1} value={settings[key]} disabled={disabled} onChange={e=>{const v=e.target.valueAsNumber;if(Number.isFinite(v)&&e.target.validity.valid){setSettings({...settings,[key]:v});setDirty(true);}}}/></label>)}</div>
 <div className="reference-actions"><label><input type="checkbox" checked={settings.invert} disabled={disabled} onChange={e=>{setSettings({...settings,invert:e.target.checked});setDirty(true);}}/>{t.invert}</label><span className="tool-spacer"/>
 <IconButton icon={busy?"refresh":"eye"} label={t.preview} active={state.image_trace&&!dirty} disabled={disabled||!state.image_trace&&state.image_editing} onClick={()=>run({type:'preview',id,settings})}/>
 <IconButton icon="check" label={t.apply} disabled={disabled||!state.image_trace||dirty} onClick={()=>run({type:'apply'})}/>
 <IconButton icon="close" label={t.cancel} disabled={busy||!state.image_trace} onClick={()=>run({type:'cancel'})}/></div>
 {error&&<div className="reference-error" role="alert">{error}</div>}
 </details>;
}
