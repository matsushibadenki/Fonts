import {convertFileSrc} from '@tauri-apps/api/core';
import {useState} from 'react';
import type {Locale,Snapshot} from './types';

const labels:Record<Locale,{empty:string;space:string;failed:string}>={
  en:{empty:'No outline',space:'Space · no outline',failed:'Preview unavailable'},
  ja:{empty:'輪郭なし',space:'空白・輪郭なし',failed:'プレビューを表示できません'},
  zh_cn:{empty:'无轮廓',space:'空格 · 无轮廓',failed:'预览不可用'},
};
type Glyph=Snapshot['glyphs'][number];
export function GlyphThumbnail({glyph,epoch,revision,locale}:{glyph:Glyph;epoch:number;revision:number;locale:Locale}){
  const [failed,setFailed]=useState(false);
  const text=labels[locale];
  if(!glyph.has_outline){
    const space=!!glyph.character&&/[\p{Z}\s]/u.test(glyph.character);
    return <span className="glyph-thumbnail empty-outline" title={space?text.space:text.empty} aria-hidden="true">{space?'␣':'◌'}</span>;
  }
  if(failed)return <span className="glyph-thumbnail preview-unavailable" title={text.failed} aria-hidden="true">⋯</span>;
  return <img className="glyph-thumbnail" src={convertFileSrc(`${epoch}-${revision}-${glyph.id}.png`,'thumbnail')} alt="" loading="lazy" decoding="async" draggable={false} onError={()=>setFailed(true)}/>;
}
