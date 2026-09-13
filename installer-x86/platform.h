#pragma once
#include "core.h"
#ifdef _WIN32
namespace install86 {
inline std::wstring psQuote(const std::wstring& v){std::wstring s=L"'";for(auto c:v){s+=c;if(c==L'\'')s+=c;}return s+L"'";}
inline std::wstring b64(const std::wstring& s){const char* alphabet="ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";std::string out;uint32_t v=0;int bits=-6;for(unsigned char c:Bytes(reinterpret_cast<const unsigned char*>(s.data()),reinterpret_cast<const unsigned char*>(s.data())+s.size()*sizeof(wchar_t))){v=(v<<8)|c;bits+=8;while(bits>=0){out+=alphabet[(v>>bits)&63];bits-=6;}}if(bits>-6)out+=alphabet[((v<<8)>>(bits+8))&63];while(out.size()%4)out+='=';return std::wstring(out.begin(),out.end());}
inline void powershell(const std::wstring& script){
    wchar_t sys[MAX_PATH]{};require(GetSystemDirectoryW(sys,MAX_PATH)!=0,"System directory unavailable");fs::path exe=fs::path(sys)/L"WindowsPowerShell/v1.0/powershell.exe";
    std::wstring cmd=L"\""+exe.wstring()+L"\" -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand "+b64(L"$ErrorActionPreference='Stop';try {"+script+L"} catch {exit 1}");
    STARTUPINFOW si{sizeof(si)};PROCESS_INFORMATION pi{};require(CreateProcessW(exe.c_str(),cmd.data(),nullptr,nullptr,FALSE,CREATE_NO_WINDOW,nullptr,nullptr,&si,&pi)!=FALSE,"Cannot start Windows PowerShell");
    CloseHandle(pi.hThread);DWORD rc=WaitForSingleObject(pi.hProcess,120000),code=1;
    if(rc!=WAIT_OBJECT_0){TerminateProcess(pi.hProcess,1);WaitForSingleObject(pi.hProcess,INFINITE);}else GetExitCodeProcess(pi.hProcess,&code);
    CloseHandle(pi.hProcess);require(rc==WAIT_OBJECT_0&&code==0,"Sidecar extraction/setup failed or timed out");
}
struct Temp{fs::path path;Temp(){path=fs::temp_directory_path()/(L"dlss5-x86-install-"+std::to_wstring(GetCurrentProcessId())+L"-"+std::to_wstring(std::chrono::steady_clock::now().time_since_epoch().count()));require(fs::create_directory(path),"Cannot create private temporary directory");}~Temp(){std::error_code ec;fs::remove_all(path,ec);}};
inline Bytes extractArchive(const fs::path& zip,const std::string& entry){
    Temp temp;auto out=temp.path/L"entry.bin";
    std::wstring name(entry.begin(),entry.end());
    powershell(L"Add-Type -AssemblyName System.IO.Compression.FileSystem;$z=[IO.Compression.ZipFile]::OpenRead("+psQuote(fs::absolute(zip).wstring())+L");try {$e=$z.GetEntry("+psQuote(name)+L");if($null -eq $e){throw 'Missing ZIP entry'};[IO.Compression.ZipFileExtensions]::ExtractToFile($e,"+psQuote(out.wstring())+L")}finally{$z.Dispose()}");
    return read(out);
}
}
#else
// Portable tests use the real pinned ZIP and zlib. Windows uses .NET ZIP support instead.
#include <zlib.h>
namespace install86 {
inline Bytes extractArchive(const fs::path& zip,const std::string& entry){
    auto b=read(zip);require(b.size()>=22,"Truncated ZIP");size_t end=b.size()-22;
    while(u32(b,end)!=0x06054b50){require(end>0&&b.size()-end<=65557,"ZIP directory absent");--end;}
    auto count=u16(b,end+10);size_t p=u32(b,end+16);
    for(unsigned i=0;i<count;++i){require(u32(b,p)==0x02014b50,"ZIP central header");auto nl=u16(b,p+28),extra=u16(b,p+30),comment=u16(b,p+32);require(p+46+nl<=b.size(),"ZIP name bounds");
        std::string name(reinterpret_cast<char*>(b.data()+p+46),nl);
        if(name==entry){auto method=u16(b,p+10);auto packed=u32(b,p+20),size=u32(b,p+24);size_t at=u32(b,p+42);require(u32(b,at)==0x04034b50,"ZIP local header");at+=30+u16(b,at+26)+u16(b,at+28);require(size<32*1024*1024&&at+packed<=b.size(),"ZIP size bounds");if(method==0)return Bytes(b.begin()+at,b.begin()+at+packed);
            require(method==8,"Unsupported ZIP compression");Bytes out(size);z_stream z{};z.next_in=b.data()+at;z.avail_in=packed;z.next_out=out.data();z.avail_out=size;require(inflateInit2(&z,-MAX_WBITS)==Z_OK,"inflate init");auto rc=inflate(&z,Z_FINISH);auto got=z.total_out;inflateEnd(&z);require(rc==Z_STREAM_END&&got==size,"ZIP inflate failed");return out;
        }p+=46+nl+extra+comment;
    }throw std::runtime_error("ZIP entry missing: "+entry);
}
}
#endif
