const words={ja:{continued:'続き',error:'ページに収まりません。文字サイズや見本の内容を調整してください。'},en:{continued:'Continued',error:'The specimen does not fit. Adjust the type size or sample content.'},zh_cn:{continued:'续页',error:'样本无法放入页面，请调整字号或示例内容。'}};
const word=words[document.documentElement.lang]||words.en;
const pages=document.getElementById('pages');
const source=document.getElementById('cards');
const printButton=document.getElementById('print');
function paginate(){
 pages.replaceChildren();
 let content;
 const nextPage=()=>{
  const page=document.createElement('section');page.className='page';
  content=document.createElement('div');content.className='page-content';
  if(!pages.children.length)content.append(document.getElementById('book-title').content.cloneNode(true));
  const footer=document.createElement('footer');footer.className='page-footer';
  const title=document.createElement('span');title.textContent=document.title;
  const number=document.createElement('span');number.className='page-number';
  footer.append(title,number);page.append(content,footer);pages.append(page);
 };
 const fits=()=>content.scrollHeight<=content.clientHeight;
 const segmenter=typeof Intl.Segmenter==='function'?new Intl.Segmenter(document.documentElement.lang.replace('_','-'),{granularity:'grapheme'}):null;
 const wordSegmenter=typeof Intl.Segmenter==='function'?new Intl.Segmenter(document.documentElement.lang.replace('_','-'),{granularity:'word'}):null;
 nextPage();
 for(const original of source.content.querySelectorAll('article')){
  const card=original.cloneNode(true);content.append(card);
  if(fits())continue;
  card.remove();
  if(content.querySelector('article')){nextPage();content.append(card);if(fits())continue;card.remove();}
  // An oversized specimen is continued with its name repeated, never clipped.
  let fragment;let continued=false;
  const begin=()=>{
   fragment=original.cloneNode(false);
   const header=original.querySelector('header');if(header)fragment.append(header.cloneNode(true));
   if(continued){const label=document.createElement('small');label.className='continuation';label.textContent=word.continued;(fragment.querySelector('header')||fragment).append(label);}
   content.append(fragment);
  };
  const advance=()=>{
   if(!fragment.querySelector('.sample,.missing,.load-error'))fragment.remove();
   continued=true;nextPage();begin();
  };
  begin();
  for(const child of original.children){
   if(child.tagName==='HEADER')continue;
   const part=child.cloneNode(true);fragment.append(part);
   if(fits())continue;
   part.remove();
   if(!child.classList.contains('sample')){advance();fragment.append(part);if(!fits())throw Error(word.error);continue;}
   let chars=segmenter?[...segmenter.segment(child.textContent)].map(s=>s.segment):Array.from(child.textContent);
   while(chars.length){
    const sample=child.cloneNode(false);fragment.append(sample);
    let low=0,high=chars.length;
    while(low<high){const mid=Math.ceil((low+high)/2);sample.textContent=chars.slice(0,mid).join('');if(fits())low=mid;else high=mid-1;}
    if(!low){sample.remove();if(!fragment.querySelector('.sample,.missing,.load-error'))throw Error(word.error);advance();continue;}
    // Prefer a word boundary near the page end instead of cutting an English word.
    let take=low;
    if(wordSegmenter&&low<chars.length){
     const end=chars.slice(0,low).join('').length;let boundary=0;
     for(const segment of wordSegmenter.segment(chars.join(''))){const n=segment.index+segment.segment.length;if(n>end)break;boundary=n;}
     if(boundary>=end*.75){let length=0;take=0;while(take<low&&length+chars[take].length<=boundary){length+=chars[take++].length;}}
    }
    sample.textContent=chars.slice(0,take).join('');chars=chars.slice(take);
    if(chars.length)advance();
   }
  }
 }
 const total=pages.children.length;
 pages.querySelectorAll('.page-number').forEach((n,i)=>n.textContent=`${i+1} / ${total}`);
 document.getElementById('page-count').textContent=`${total}`;
 document.documentElement.dataset.pages=String(total);
}
(async()=>{
 try{
  for(const a of source.content.querySelectorAll('article')){
   try{await document.fonts.load('24px '+a.dataset.font);if(!document.fonts.check('24px '+a.dataset.font))throw Error();}
   catch{a.classList.add('failed');const p=document.createElement('p');p.className='load-error';p.textContent=document.getElementById('load-error-label').textContent;a.append(p);}
  }
  for(const e of source.content.querySelectorAll('.missing-label'))e.textContent=document.getElementById('missing-label').textContent;
  await document.fonts.ready;paginate();printButton.disabled=false;
 }catch(e){document.getElementById('layout-error').textContent=e.message;}
})();
printButton.onclick=()=>window.print();
