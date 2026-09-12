#pragma once
#include <windows.h>
#include "bridge_ipc.h"
namespace x86bridge {
struct Handle {
    HANDLE value=nullptr;
    Handle()=default;explicit Handle(HANDLE h):value(h){}
    Handle(const Handle&)=delete;Handle& operator=(const Handle&)=delete;
    ~Handle(){reset();}
    void reset(HANDLE h=nullptr){if(value&&value!=INVALID_HANDLE_VALUE)CloseHandle(value);value=h;}
    explicit operator bool()const{return value&&value!=INVALID_HANDLE_VALUE;}
};
// CPU barrier only. Wait for this I/O or peer process death, never an old frame.
// Cancelled I/O is retired before the stack OVERLAPPED/buffer can disappear.
inline bool Transfer(HANDLE pipe,HANDLE peer,void* data,uint32_t size,bool write){
    auto* at=static_cast<unsigned char*>(data);
    while(size){
        Handle ev(CreateEventW(nullptr,TRUE,FALSE,nullptr));if(!ev)return false;
        OVERLAPPED ov{};ov.hEvent=ev.value;DWORD n=0;
        BOOL ok=write?WriteFile(pipe,at,size,&n,&ov):ReadFile(pipe,at,size,&n,&ov);
        if(!ok){
            if(GetLastError()!=ERROR_IO_PENDING)return false;
            HANDLE waits[]={ev.value,peer};const DWORD rc=WaitForMultipleObjects(2,waits,FALSE,INFINITE);
            if(rc!=WAIT_OBJECT_0){CancelIoEx(pipe,&ov);GetOverlappedResult(pipe,&ov,&n,TRUE);return false;}
            if(!GetOverlappedResult(pipe,&ov,&n,FALSE))return false;
        }
        if(!n||n>size)return false;at+=n;size-=n;
    }
    return true;
}
inline bool Send(HANDLE p,HANDLE peer,const void* b,uint32_t n){return Transfer(p,peer,const_cast<void*>(b),n,true);}
inline bool Receive(HANDLE p,HANDLE peer,void* b,uint32_t n){return Transfer(p,peer,b,n,false);}
inline bool Request(HANDLE p,HANDLE peer,Kind k,const void* body,uint32_t bytes,Ack& ack){
    Header h;h.kind=k;h.bytes=bytes;
    return ValidHeader(h)&&Send(p,peer,&h,sizeof(h))&&(!bytes||Send(p,peer,body,bytes))&&Receive(p,peer,&ack,sizeof(ack))&&ack.header.magic==Magic&&ack.header.version==Version&&ack.header.kind==k&&ack.header.bytes==sizeof(Ack)-sizeof(Header);
}
}
