import type { IslandViewName } from '../core/layout';
export interface ViewHost { el:HTMLElement; sync():void; focus?():void; tick?(nowMs:number):boolean|void; }
export interface ViewActions { setView(v:IslandViewName):void; }
