import {useState} from 'react';
import {IconButton} from './Panel';
import {Icon} from './icons';
import type {IconName} from './icons';
import type {Layout,Panel,Snapshot,SplitSide} from './types';
import type {Key} from './i18n';
export const splitOptions:{side:SplitSide;key:Key;icon:IconName}[]=[
  {side:'left',key:'splitLeft',icon:'splitLeft'},{side:'right',key:'splitRight',icon:'splitRight'},
  {side:'top',key:'splitTop',icon:'splitTop'},{side:'bottom',key:'splitBottom',icon:'splitBottom'},
  {side:'tab',key:'tab',icon:'tab'},
];
export function PanelArrangement({id,state,t,layout}:{id:Panel;state:Snapshot;t:(k:Key)=>string;layout:(a:Layout)=>Promise<void>}){
  const [target,setTarget]=useState<Panel|null>(null);
  const targets=(Object.keys(state.workspace.panels) as Panel[]).filter(p=>p!==id&&state.workspace.panels[p].kind==='docked');
  const selected=target&&targets.includes(target)?target:targets[0];
  return <div className="panel-arrangement">
    <select aria-label={t('panelTarget')} value={selected??''} disabled={!selected} onChange={e=>setTarget(e.target.value as Panel)}>{targets.map(p=><option key={p} value={p}>{t(p)}</option>)}</select>
    <span className="panel-help" role="img" tabIndex={0} title={t('splitHelp')} aria-label={t('splitHelp')}><Icon name="info"/></span>
    <div className="arrangement-directions">{splitOptions.map(o=><IconButton key={o.side} icon={o.icon} label={t(o.key)} disabled={!selected} onClick={()=>{if(selected)void layout({type:'relative',panel:id,target:selected,side:o.side});}}/>)}</div>
  </div>;
}
