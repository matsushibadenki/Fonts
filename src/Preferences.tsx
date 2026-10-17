import {useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {IconButton} from './Panel';
import {Icon} from './icons';
import type {Locale,Snapshot} from './types';
const labels={
  en:{title:'Preferences',general:'General',workspace:'Workspace',language:'Language',close:'Close',reset:'Reset panel layout',help:'Language changes apply immediately and are saved for the next launch.',layoutHelp:'Returns panels to their default docks and sizes.',error:'Could not update preferences. Try again.'},
  ja:{title:'環境設定',general:'一般',workspace:'ワークスペース',language:'表示言語',close:'閉じる',reset:'パネル配置を戻す',help:'言語の変更はすぐに反映され、次回起動時にも保持されます。',layoutHelp:'パネルの配置とサイズを初期状態に戻します。',error:'設定を更新できませんでした。再度お試しください。'},
  zh_cn:{title:'偏好设置',general:'常规',workspace:'工作区',language:'显示语言',close:'关闭',reset:'重置面板布局',help:'语言更改会立即生效，并在下次启动时保留。',layoutHelp:'将面板恢复为默认停靠位置和尺寸。',error:'无法更新设置，请重试。'},
};
const tabs=['general','workspace'] as const;
type Tab=typeof tabs[number];
export function Preferences({state,onOpenChange}:{state:Snapshot;onOpenChange:(v:boolean)=>void}){
  const dialog=useRef<HTMLDialogElement>(null);
  const pending=useRef(false);
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState(false);
  const [tab,setTab]=useState<Tab>('general');
  const t=labels[state.workspace.locale];
  const change=async(command:string,args:Record<string,unknown>)=>{
    if(pending.current)return;
    pending.current=true;setBusy(true);setError(false);
    try{await invoke(command,args);}catch{setError(true);}finally{pending.current=false;setBusy(false);}
  };
  const selectTab=(next:Tab)=>{setTab(next);setError(false);};
  return <>
    <IconButton icon="settings" label={t.title} onClick={()=>{setError(false);dialog.current?.showModal();onOpenChange(true);}}/>
    <dialog ref={dialog} className="preferences-dialog" aria-labelledby="preferences-title" onClose={()=>onOpenChange(false)} onCancel={e=>{e.stopPropagation();if(pending.current)e.preventDefault();}} onKeyDown={e=>e.stopPropagation()}>
      <header><strong id="preferences-title">{t.title}</strong><IconButton icon="close" label={t.close} disabled={busy} onClick={()=>dialog.current?.close()}/></header>
      <div className="preferences-tabs" role="tablist" aria-label={t.title}>
        {tabs.map((id,i)=><button key={id} id={`preferences-tab-${id}`} type="button" role="tab" aria-selected={tab===id} aria-controls={`preferences-panel-${id}`} tabIndex={tab===id?0:-1} onClick={()=>selectTab(id)} onKeyDown={e=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();const next=e.key==='Home'?0:e.key==='End'?tabs.length-1:(i+(e.key==='ArrowRight'?1:tabs.length-1))%tabs.length;selectTab(tabs[next]);document.getElementById(`preferences-tab-${tabs[next]}`)?.focus();}}>{t[id]}</button>)}
      </div>
      <section id={`preferences-panel-${tab}`} role="tabpanel" aria-labelledby={`preferences-tab-${tab}`} className="preferences-content">
        {tab==='general'?<label className="preferences-row"><span>{t.language}<span className="panel-help" tabIndex={0} role="img" title={t.help} aria-label={t.help}><Icon name="info"/></span></span><select aria-label={t.language} value={state.workspace.locale} disabled={busy} onChange={e=>{const locale=e.target.value as Locale;if(locale!==state.workspace.locale)void change('edit',{epoch:state.epoch,action:{type:'locale',locale}});}}><option value="en">English</option><option value="ja">日本語</option><option value="zh_cn">简体中文</option></select></label>:
          <div className="preferences-row"><button type="button" className="preferences-reset" disabled={busy} onClick={()=>void change('layout',{revision:state.workspace.revision,action:{type:'reset'}})}><Icon name="reset"/>{t.reset}</button><span className="panel-help" tabIndex={0} role="img" title={t.layoutHelp} aria-label={t.layoutHelp}><Icon name="info"/></span></div>}
        {error&&<p role="alert">{t.error}</p>}
      </section>
    </dialog>
  </>;
}
