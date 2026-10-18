export type Panel = 'glyphs'|'metrics'|'outline'|'snippets';
export type SplitSide = 'left'|'right'|'top'|'bottom'|'tab';
export type DockNode = {kind:'tabs';panels:Panel[];active:Panel}|{kind:'split';axis:'horizontal'|'vertical';ratio:number;first:DockNode;second:DockNode};
export type Dock = 'left'|'right'|'bottom';
export type Locale = 'en'|'ja'|'zh_cn';
export type Tool = 'nodes'|'pen'|'pan'|'images'|'snippets';
export interface ReferenceImage {id:number;asset_id:string;center:[number,number];width:number;height:number;angle:number;opacity:number;visible:boolean;locked:boolean}
export type Placement = {kind:'docked';dock:Dock}|{kind:'floating';return_to:Dock};
export interface FontMetadata {style:string;weight:number;width:number;italic_angle:number;bold:boolean;italic:boolean;copyright:string;license:string;license_url:string;cid_system:{registry:string;ordering:string;supplement:number}|null}
export interface FontProperties {family:string;metadata:FontMetadata}
export interface ProofSettings {kerning:boolean;text:string;size:number;tracking:number;leading:number;scroll:number}
export interface SnippetInfo {name:string;tags:string;category:string;source:string;license:string;favorite:boolean}
export interface SnippetEntry {key:string;builtin:boolean;info:SnippetInfo;contours:number}
export interface KerningClass {id:number;name:string;glyphs:number[]}
export interface ClassKerning {next_id:number;left:KerningClass[];right:KerningClass[];pairs:{left:number;right:number;value:number}[]}
export interface Snapshot {
  document_id:number;documents:{id:number;family:string;path:string|null;dirty:boolean}[];
  class_kerning:ClassKerning;
  kerning:{left:number;right:number;value:number}[];kerning_total:number;
  snippets:{entries:SnippetEntry[];selected:string|null;contours:number[];placement:ReferenceImage|null};
  images:ReferenceImage[];image_selected:number|null;image_editing:boolean;
  selection:{contour:number;anchor:number;smooth:boolean;can_smooth:boolean}|null;
  properties:FontProperties;presets:[string,number][];
  proof:{color:boolean;source:string|null;active:boolean;settings:ProofSettings;missing:number[];empty:number;comparison:boolean;height:number};
  family:string;epoch:number;revision:number;glyph:number;character:string;codepoints:number[];name:string;cid:number|null;advance:number;ascender:number;descender:number;upm:number;unencoded:number;cid_system:{registry:string;ordering:string;supplement:number}|null;
  glyphs:{id:number;character:string;codepoints:number[];name:string;cid:number|null;reviewed:boolean;has_outline:boolean}[];
  contours:{nodes:number;closed:boolean;start:[number,number];segments:{end:[number,number];deletable:boolean;closure:boolean}[]}[];
  workspace:{trees:Partial<Record<Dock,DockNode>>;schema_version:number;revision:number;locale:Locale;panels:Record<Panel,Placement>;active:Partial<Record<Dock,Panel>>;left_width:number;right_width:number;bottom_height:number};
  tool:Tool;zoom:number;nodes:boolean;fill:boolean;can_undo:boolean;can_redo:boolean;dirty:boolean;reviewed:boolean;
}
export type Edit = {type:string;[key:string]:unknown};
export type Layout = {type:'relative';panel:Panel;target:Panel;side:SplitSide}|{type:'resize_split';dock:Dock;path:number[];value:number}|{type:'float'|'activate';panel:Panel}|{type:'dock';panel:Panel;dock:Dock}|{type:'resize';dock:Dock;value:number}|{type:'reset'};
