export type Panel = 'glyphs'|'metrics'|'outline';
export type Dock = 'left'|'right'|'bottom';
export type Locale = 'en'|'ja'|'zh_cn';
export type Tool = 'nodes'|'pen'|'pan'|'images';
export interface ReferenceImage {id:number;asset_id:string;center:[number,number];width:number;height:number;angle:number;opacity:number;visible:boolean;locked:boolean}
export type Placement = {kind:'docked';dock:Dock}|{kind:'floating';return_to:Dock};
export interface FontMetadata {style:string;weight:number;width:number;italic_angle:number;bold:boolean;italic:boolean;copyright:string;license:string;license_url:string;cid_system:{registry:string;ordering:string;supplement:number}|null}
export interface FontProperties {family:string;metadata:FontMetadata}
export interface ProofSettings {text:string;size:number;tracking:number;leading:number;scroll:number}
export interface Snapshot {
  images:ReferenceImage[];image_selected:number|null;image_editing:boolean;
  selection:{contour:number;anchor:number;smooth:boolean;can_smooth:boolean}|null;
  properties:FontProperties;presets:[string,number][];
  proof:{active:boolean;settings:ProofSettings;missing:number[];empty:number;comparison:boolean;height:number};
  family:string;epoch:number;revision:number;glyph:number;character:string;codepoints:number[];name:string;cid:number|null;advance:number;ascender:number;descender:number;upm:number;unencoded:number;cid_system:{registry:string;ordering:string;supplement:number}|null;
  glyphs:{id:number;character:string;codepoints:number[];name:string;cid:number|null;reviewed:boolean;has_outline:boolean}[];
  contours:{nodes:number;closed:boolean;start:[number,number];segments:{end:[number,number];deletable:boolean;closure:boolean}[]}[];
  workspace:{schema_version:number;revision:number;locale:Locale;panels:Record<Panel,Placement>;active:Partial<Record<Dock,Panel>>;left_width:number;right_width:number;bottom_height:number};
  tool:Tool;zoom:number;nodes:boolean;fill:boolean;can_undo:boolean;can_redo:boolean;dirty:boolean;reviewed:boolean;
}
export type Edit = {type:string;[key:string]:unknown};
export type Layout = {type:'float'|'activate';panel:Panel}|{type:'dock';panel:Panel;dock:Dock}|{type:'resize';dock:Dock;value:number}|{type:'reset'};
