import {spawn, type ChildProcessWithoutNullStreams, type SpawnOptionsWithoutStdio} from 'node:child_process';
import {EventEmitter, once} from 'node:events';
export const PROTOCOL_VERSION=1;
export const MAX_FRAME_BYTES=1024*1024;
export type Json=null|boolean|number|string|Json[]|{[key:string]:Json};
export type ToolMode='host'|'local';
export interface ToolDefinition{name:string;description?:string;inputSchema:{[key:string]:Json}}
export interface SessionOptions{
  cwd:string;toolMode?:ToolMode;tools?:ToolDefinition[];instructions?:string;model?:string;
  connection?:string;provider?:string;baseUrl?:string;apiKeyEnv?:string|null;
  imageInput?:boolean;reasoningBudget?:number;maxTokens?:number;
}
export type ContextBlock={type:'text';text:string}|{type:'resource';uri:string;text:string}|{type:'json';value:Json};
export interface ImageInput{mediaType:'image/png'|'image/jpeg'|'image/webp'|'image/gif';data:string}
export interface TurnInput{text?:string;context?:ContextBlock[];images?:ImageInput[]}
export type ContentBlock={type:'text';text:string}|{type:'json';value:Json}|({type:'image'}&ImageInput);
export interface ToolResult{text?:string;isError?:boolean;content?:ContentBlock[];structuredContent?:Json}
export interface ToolRequest{sessionId:string;turnId:string|number;callId:string;name:string;arguments:Json}
export interface RuntimeEvent{sessionId:string;turnId:string|number;type:string;data:{[key:string]:Json}}
export interface Entry{entryId:string;cursor:number;record:{k:string;[key:string]:Json};truncatedFields:{field:string;bytes:number}[]}
export interface EntryPage{entries:Entry[];cursor:number;hasMore:boolean;journalBytes:number}
export interface CompletionResult{text:string;stopReason:'completed'|'cancelled';usage:Json}
export class RuntimeError extends Error{
  constructor(message:string,public readonly code:number,public readonly data:Json=null){super(message);this.name='RuntimeError'}
}
interface Pending{resolve:(value:unknown)=>void;reject:(error:Error)=>void}
export class RuntimeClient extends EventEmitter{
  readonly child:ChildProcessWithoutNullStreams;
  private readonly pending=new Map<string,Pending>();
  private sequence=0;
  private buffer=Buffer.alloc(0);
  private ended=false;
  private writes:Promise<void>=Promise.resolve();
  private queuedWrites=0;
  constructor(binary='gc',options:SpawnOptionsWithoutStdio={}){
    super();
    this.child=spawn(binary,['runtime'],{...options,stdio:'pipe'});
    this.child.stderr.on('data',(data:Buffer)=>this.emit('diagnostic',data.toString('utf8')));
    this.child.stdout.on('data',(data:Buffer)=>this.read(data));
    this.child.stdin.on('error',(error:Error)=>this.fail(error));
    this.child.on('error',(error:Error)=>this.fail(error));
    this.child.on('close',(code)=>{
      if(code===0 && this.pending.size===0){this.ended=true;this.emit('closed')}
      else this.fail(new Error('runtime disconnected'));
    });
  }
  private fail(error:Error){
    if(this.ended)return;
    this.ended=true;
    for(const item of this.pending.values())item.reject(error);
    this.pending.clear();
    this.emit('runtimeError',error);
    this.emit('closed');
  }
  private read(data:Buffer){
    this.buffer=Buffer.concat([this.buffer,data]);
    for(;;){
      const end=this.buffer.indexOf(10);
      if(end<0)break;
      if(end+1>MAX_FRAME_BYTES){this.fail(new Error('runtime frame exceeds 1 MiB'));this.child.kill();return}
      const line=this.buffer.subarray(0,end);
      this.buffer=this.buffer.subarray(end+1);
      try{
        const value=JSON.parse(line.toString('utf8')) as Record<string,unknown>;
        if(value['jsonrpc']!=='2.0')throw new Error('invalid JSON-RPC envelope');
        if(value['method']==='runtime/event'){
          this.emit('runtimeEvent',value['params'] as RuntimeEvent);
        }else if(value['method']==='tool/execute'){
          if(this.listenerCount('toolRequest')===0){
            void this.toolResult(value['id'] as string,{text:'Host has no tool executor',isError:true}).catch(error=>this.fail(error as Error));
          }else this.emit('toolRequest',value['id'],value['params'] as ToolRequest);
        }else if(typeof value['id']==='string'){
          const pending=this.pending.get(value['id']);if(!pending){this.emit('unmatchedResponse',value);continue}
          this.pending.delete(value['id']);
          if(value['error']){
            const error=value['error'] as {code:number;message:string;data?:Json};
            pending.reject(new RuntimeError(error.message,error.code,error.data??null));
          }else if(Object.hasOwn(value,'result'))pending.resolve(value['result']);
          else pending.reject(new Error('invalid response envelope'));
        }else this.emit('protocolMessage',value);
      }catch(error){this.fail(error as Error);this.child.kill();return}
    }
    if(this.buffer.length>MAX_FRAME_BYTES){this.fail(new Error('runtime frame exceeds 1 MiB'));this.child.kill()}
  }
  private send(value:unknown):Promise<void>{
    if(this.ended)return Promise.reject(new Error('runtime disconnected'));
    const data=Buffer.from(JSON.stringify(value)+'\n');
    if(data.length>MAX_FRAME_BYTES)return Promise.reject(new Error('runtime frame exceeds 1 MiB'));
    if(this.queuedWrites>=64)return Promise.reject(new Error('runtime write queue is full'));
    this.queuedWrites++;
    const write=this.writes.then(async()=>{
      if(this.ended)throw new Error('runtime disconnected');
      if(!this.child.stdin.write(data))await once(this.child.stdin,'drain');
    });
    this.writes=write.catch(()=>{}).finally(()=>{this.queuedWrites--});
    return write;
  }
  request<T=Json>(method:string,params:unknown):Promise<T>{
    if(this.pending.size>=64)return Promise.reject(new Error('runtime request limit reached'));
    const id='ts-'+(++this.sequence);
    return new Promise<T>((resolve,reject)=>{
      this.pending.set(id,{resolve:value=>resolve(value as T),reject});
      void this.send({jsonrpc:'2.0',id,method,params}).catch(error=>{this.pending.delete(id);reject(error as Error)});
    });
  }
  initialize(storageDir:string){return this.request('initialize',{protocolVersion:PROTOCOL_VERSION,storageDir})}
  newSession(options:SessionOptions){return this.request<{sessionId:string}>('session/new',options)}
  resumeSession(sessionId:string,expectedToolMode:ToolMode='host'){return this.request<{sessionId:string}>('session/resume',{sessionId,expectedToolMode})}
  prompt(sessionId:string,input:TurnInput){return this.request('turn/prompt',{sessionId,...input})}
  steer(sessionId:string,input:TurnInput,turnId?:string|number){return this.request('turn/steer',{sessionId,...input,...(turnId===undefined?{}:{turnId})})}
  cancel(sessionId:string,turnId?:string|number){return this.request('turn/cancel',{sessionId,...(turnId===undefined?{}:{turnId})})}
  entries(sessionId:string,cursor=0,limit=25){return this.request<EntryPage>('session/entries',{sessionId,cursor,limit})}
  complete(sessionId:string,prefix:string,suffix:string,maxTokens=1024){return this.request<CompletionResult>('completion/request',{sessionId,prefix,suffix,maxTokens})}
  toolResult(requestId:string,result:ToolResult){return this.send({jsonrpc:'2.0',id:requestId,result})}
  progress(sessionId:string,requestId:string,text:string){return this.send({jsonrpc:'2.0',method:'tool/progress',params:{sessionId,requestId,text}})}
  acknowledgeCancel(sessionId:string,requestId:string,executionStatus:'cancelled'|'completed'|'unknown'){
    return this.request('tool/cancelled',{sessionId,requestId,executionStatus});
  }
  async shutdown(){await this.request('shutdown',{})}
  close(){this.child.stdin.end();const timer=setTimeout(()=>this.child.kill(),3000);timer.unref();this.child.once('exit',()=>clearTimeout(timer))}
}
