#pragma once
// Additive x86 installer. No game identities or neural/GPU code.
#include <filesystem>
#include <fstream>
#include <sstream>
#include <vector>
#include <map>
#include <set>
#include <regex>
#include <algorithm>
#include <stdexcept>
#include <chrono>
#include <cstdint>
#include <cctype>
#include <iomanip>
#ifdef _WIN32
#include <windows.h>
#include <bcrypt.h>
#else
#include <openssl/sha.h>
#endif
namespace install86 {
namespace fs=std::filesystem;
using Bytes=std::vector<unsigned char>;
inline constexpr const char* Notice="Install official ReShade Full Add-on Support for the translated API. D3D8 uses the pinned d3d8to9 compatibility layer and the native D3D9 frontend.";
inline constexpr const char* RuntimeSha="ddd82d313aa74c2e7602d17dfb7e7cd90cca9bfc0306f581684d35d75d1b350b";
inline constexpr const char* WeightsSha="6bf8dc931ef3ccffe18c82de26ab374156e7f19539ffcf8eabaa25dca5cf15ab";
inline constexpr const char* ReShadeSha="da430e0a9c6eecefa0d1b27d05e16c426fb5d04e808b194d914eaac4b31bc0f8";
inline constexpr const char* D3D8To9Version="v1.15.1";
inline constexpr const char* D3D8To9Commit="65870f2302e9c496cd6d873d6095961d5c777668";
inline constexpr const char* D3D8To9Sha="ab6bf7a9a9f4b3e66a75ca038d8d10289c88acbfe8d52c3b5a8a9a259cb26cd5";
inline constexpr const char* ManifestName="dlss5-x86bridge.install.json";
inline void require(bool b,const std::string& why){if(!b)throw std::runtime_error(why);}
inline std::string lower(std::string s){for(auto& c:s)c=static_cast<char>(std::tolower(static_cast<unsigned char>(c)));return s;}
inline std::string trim(std::string s){auto a=s.find_first_not_of(" \t\r\n");return a==s.npos?"":s.substr(a,s.find_last_not_of(" \t\r\n")-a+1);}
inline Bytes read(const fs::path& p){std::ifstream f(p,std::ios::binary);require(bool(f),"Cannot read "+p.string());return Bytes(std::istreambuf_iterator<char>(f),{});}
inline std::string str(const Bytes& b){return std::string(b.begin(),b.end());}
inline Bytes bytes(const std::string& s){return Bytes(s.begin(),s.end());}
inline void write(const fs::path& p,const Bytes& b){std::ofstream f(p,std::ios::binary|std::ios::trunc);require(bool(f),"Cannot write "+p.string());f.write(reinterpret_cast<const char*>(b.data()),b.size());f.close();require(!f.fail(),"Write failed "+p.string());}
inline std::string sha(const Bytes& b){unsigned char digest[32]{};
#ifdef _WIN32
    BCRYPT_ALG_HANDLE alg=nullptr;BCRYPT_HASH_HANDLE hash=nullptr;
    require(BCryptOpenAlgorithmProvider(&alg,BCRYPT_SHA256_ALGORITHM,nullptr,0)>=0,"SHA provider");
    DWORD objectSize=0,n=0;auto rc=BCryptGetProperty(alg,BCRYPT_OBJECT_LENGTH,reinterpret_cast<PUCHAR>(&objectSize),sizeof(objectSize),&n,0);
    std::vector<unsigned char> object(objectSize);
    if(rc>=0)rc=BCryptCreateHash(alg,&hash,object.data(),objectSize,nullptr,0,0);
    if(rc>=0)rc=BCryptHashData(hash,const_cast<PUCHAR>(b.data()),static_cast<ULONG>(b.size()),0);
    if(rc>=0)rc=BCryptFinishHash(hash,digest,32,0);
    if(hash)BCryptDestroyHash(hash);BCryptCloseAlgorithmProvider(alg,0);require(rc>=0,"SHA operation");
#else
    SHA256(b.data(),b.size(),digest);
#endif
    std::ostringstream s;for(auto v:digest)s<<std::hex<<std::setw(2)<<std::setfill('0')<<unsigned(v);return s.str();
}
inline std::string hashFile(const fs::path& p){return sha(read(p));}
inline void hashIs(const Bytes& b,const std::string& expected,const std::string& what){require(sha(b)==expected,what+" SHA256 mismatch");}
inline uint16_t u16(const Bytes& b,size_t p){require(p+2<=b.size(),"Truncated PE");return uint16_t(b[p]|(b[p+1]<<8));}
inline uint32_t u32(const Bytes& b,size_t p){return uint32_t(u16(b,p))|(uint32_t(u16(b,p+2))<<16);}
inline uint16_t machine(const Bytes& b){require(b.size()>=64&&u16(b,0)==0x5a4d,"Not a PE executable");auto p=u32(b,60);require(p<=b.size()&&b.size()-p>=26&&u32(b,p)==0x4550,"Invalid PE header");auto m=u16(b,p+4);require((m==0x14c&&u16(b,p+24)==0x10b)||(m==0x8664&&u16(b,p+24)==0x20b),"Unsupported PE format");return m;}
// Some maintained game wrappers deliberately forward Direct3D 8 to d3d8R.dll. Detect only an
// explicit embedded sidecar name; otherwise fail closed rather than replacing an unknown wrapper.
inline bool advertisesD3D8Sidecar(const Bytes& b){
    constexpr const char* marker="d3d8r.dll";constexpr size_t n=9;
    for(size_t i=0;i+n<=b.size();++i){bool ascii=true,utf16=i+n*2<=b.size();for(size_t j=0;j<n;++j){
        ascii=ascii&&std::tolower(static_cast<unsigned char>(b[i+j]))==marker[j];
        utf16=utf16&&std::tolower(static_cast<unsigned char>(b[i+j*2]))==marker[j]&&b[i+j*2+1]==0;
    }if(ascii||utf16)return true;}return false;
}
// Preserve all unedited INI bytes, including comments and unrelated preferences.
inline std::string getIni(const std::string& s,const std::string& section,const std::string& key){std::istringstream in(s);std::string line,sec;while(std::getline(in,line)){auto t=trim(line);if(t.size()>1&&t[0]=='['&&t.back()==']')sec=lower(t.substr(1,t.size()-2));else if(sec==lower(section)){auto eq=t.find('=');if(eq!=t.npos&&lower(trim(t.substr(0,eq)))==lower(key))return t.substr(eq+1);}}return "";}
inline std::string setIni(std::string s,const std::string& section,const std::string& key,const std::string& value){
    std::string sec;size_t start=0,insert=s.npos;bool found=false;const std::string nl=s.find("\r\n")!=s.npos?"\r\n":"\n";
    while(start<s.size()){
        auto end=s.find('\n',start);if(end==s.npos)end=s.size();else ++end;
        auto t=trim(s.substr(start,end-start));
        if(t.size()>1&&t.front()=='['&&t.back()==']'){
            if(found&&sec==lower(section)){insert=start;break;}
            sec=lower(t.substr(1,t.size()-2));if(sec==lower(section)){found=true;insert=end;}
        }else if(sec==lower(section)){
            auto eq=t.find('=');if(eq!=t.npos&&lower(trim(t.substr(0,eq)))==lower(key)){s.replace(start,end-start,key+"="+value+nl);return s;}
            insert=end;
        }start=end;
    }
    if(found){s.insert(insert,(insert&&s[insert-1]!='\n'?nl:"")+key+"="+value+nl);return s;}
    if(!s.empty()&&s.back()!='\n')s+=nl;
    return s+nl+"["+section+"]"+nl+key+"="+value+nl;
}
inline std::string freshIni(){return "[dlss5]\r\n; x86 fresh-install overrides. All other values follow upstream defaults.\r\nScale=1.0\r\nColourStrength=0.25\r\nStructure=1\r\nSkin=1\r\nPasses=1\r\n";}
inline std::string firstDock(std::string ini,unsigned width,unsigned height){
    auto windows=getIni(ini,"OVERLAY","Window");
    const std::string panel="[Window][DLSS Neural Rendering (AMD)]";
    if(windows.find(panel)!=windows.npos)return ini; // Even undocked saved state is authoritative.
    std::string dock;auto home=windows.find("[Window][###home]");
    if(home!=windows.npos){auto end=windows.find("[Window]",home+1);auto chunk=windows.substr(home,end==windows.npos?windows.npos:end-home);std::smatch m;if(std::regex_search(chunk,m,std::regex("DockId=(0x[0-9A-Fa-f]+)")))dock=m[1];}
    if(dock.empty()){
        // Respect an existing layout without a docked Home. No destructive rebuild or guessed target.
        if(!windows.empty()||!getIni(ini,"OVERLAY","Docking").empty())return ini;
        require(width>=320&&height>=240,"Viewport size unavailable for fresh docking");
        const unsigned left=width*35/100;
        const std::string layout="[Docking][Data],DockSpace   ID=0xB0DF600F Pos=0,,0 Size="+std::to_string(width)+",,"+std::to_string(height)+" Split=X,  DockNode  ID=0x00000001 Parent=0xB0DF600F SizeRef="+std::to_string(left)+",,"+std::to_string(height)+",  DockNode  ID=0x00000002 Parent=0xB0DF600F SizeRef="+std::to_string(width-left)+",,"+std::to_string(height)+" CentralNode=1";
        ini=setIni(ini,"OVERLAY","Docking",layout);dock="0x00000001";
        unsigned tab=0;for(auto title:{"###home","###addons","###settings","###statistics","###log","###about"}){if(!windows.empty())windows+=",";windows+="[Window]["+std::string(title)+"],Collapsed=0,DockId="+dock+",,"+std::to_string(tab++);}
    }
    if(!windows.empty())windows+=",";
    windows+=panel+",Collapsed=0,DockId="+dock;return setIni(ini,"OVERLAY","Window",windows);
}
inline const std::set<std::string>& allowed(){static const std::set<std::string> a={"dxgi.dll","d3d8.dll","d3d8R.dll","d3d9.dll","dgVoodoo.conf","ReShade.ini","dlss5-neural.ini","dlss5-neural.addon32","dlss5-neural-host64.exe","dlssnr_amd_pass1.dll","dlssnr_on_amd_weights.bin"};return a;}
inline bool config(const std::string& n){return n=="ReShade.ini"||n=="dgVoodoo.conf"||n=="dlss5-neural.ini";}
inline void safePath(const fs::path& p){
    fs::path walk;for(const auto& part:fs::absolute(p)){walk/=part;
        require(!fs::is_symlink(fs::symlink_status(walk)),"Symbolic link refused: "+walk.string());
#ifdef _WIN32
        auto attrs=GetFileAttributesW(walk.c_str());require(attrs==INVALID_FILE_ATTRIBUTES||!(attrs&FILE_ATTRIBUTE_REPARSE_POINT),"Reparse path refused");
#endif
    }
}
inline fs::path installDirectory(const fs::path& target){
    const auto root=fs::weakly_canonical(fs::absolute(target).parent_path());safePath(root);
    const auto redirect=root/L"ReShade.ini";safePath(redirect);
    if(!fs::exists(redirect))return root;
    const auto configured=trim(getIni(str(read(redirect)),"INSTALL","BasePath"));
    if(configured.empty())return root;
    auto candidate=fs::path(configured);if(candidate.is_relative())candidate=root/candidate;
    candidate=fs::weakly_canonical(candidate);safePath(candidate);
    const auto relative=candidate.lexically_relative(root);
    require(!relative.empty()&&*relative.begin()!=fs::path(".."),"ReShade BasePath must stay inside the selected game directory");
    require(fs::is_directory(candidate),"ReShade BasePath is not an existing directory");return candidate;
}
struct Entry{std::string name,hash,backup,backupHash;bool owned=false,configuration=false;};
struct Manifest{std::string preset,state="installed";std::vector<Entry> entries;};
inline std::string encode(const Manifest& m){std::ostringstream o;o<<"{\n\"schema\":1,\n\"preset\":\""<<m.preset<<"\",\n\"state\":\""<<m.state<<"\",\n\"bridge_protocol\":2,\n\"dgVoodoo\":\"none\",\n\"ReShade\":\"6.8.0.2156 Full Add-on Support\",\n\"files\":[\n";for(size_t i=0;i<m.entries.size();++i){auto& e=m.entries[i];o<<"{\"name\":\""<<e.name<<"\",\"sha256\":\""<<e.hash<<"\",\"backup\":\""<<e.backup<<"\",\"backup_sha256\":\""<<e.backupHash<<"\",\"owned\":"<<(e.owned?"true":"false")<<",\"configuration\":"<<(e.configuration?"true":"false")<<"}"<<(i+1==m.entries.size()?"":",")<<"\n";}return o.str()+"]\n}\n";}
inline Manifest decode(const std::string& s){
    Manifest m;std::smatch v;require(std::regex_search(s,v,std::regex("\"schema\":1,")),"Unknown manifest schema");
    require(std::regex_search(s,v,std::regex("\"preset\":\"(D3D11|D3D9|D3D8)\"")),"Bad manifest preset");m.preset=v[1];
    require(std::regex_search(s,v,std::regex("\"state\":\"(installed|installing)\"")),"Bad manifest state");m.state=v[1];
    std::regex row(R"re(\{"name":"([^"]+)","sha256":"([0-9a-f]{64})","backup":"([^"]*)","backup_sha256":"([0-9a-f]*)","owned":(true|false),"configuration":(true|false)\})re");
    std::set<std::string> seen;
    for(std::sregex_iterator i(s.begin(),s.end(),row),end;i!=end;++i){auto x=*i;Entry e{x[1],x[2],x[3],x[4],x[5]=="true",x[6]=="true"};
        require(allowed().count(e.name)&&seen.insert(e.name).second,"Unsafe/duplicate manifest entry");
        require(e.configuration==config(e.name),"Manifest config mismatch");
        if(!e.backup.empty())require(std::regex_match(e.backup,std::regex("\\.dlss5-x86bridge-backups/[0-9]+/"+e.name))&&e.backupHash.size()==64&&e.owned,"Unsafe backup entry");
        if(!e.backup.empty())require(fs::path(e.backup).filename()==e.name,"Backup filename mismatch");
        m.entries.push_back(e);
    }
    size_t rows=0,pos=0;while((pos=s.find("\"name\":",pos))!=s.npos){++rows;pos+=7;}require(rows==m.entries.size()&&rows<=allowed().size(),"Malformed manifest entries");
    auto canonical=encode(m);bool supported=canonical==s;
    // The original fork used dgVoodoo for both translated presets. Preserve its manifests for
    // uninstall/recovery, but never create another one or carry that wrapper into a new install.
    if(!supported&&(m.preset=="D3D8"||m.preset=="D3D9")){
        const std::string current="\"dgVoodoo\":\"none\"",legacy="\"dgVoodoo\":\"2.87.4\"";
        const auto at=canonical.find(current);if(at!=canonical.npos)canonical.replace(at,current.size(),legacy);
        supported=canonical==s;
    }
    require(supported,"Modified or unsupported install manifest");return m;
}
inline void atomicManifest(const fs::path& dir,const Manifest& m){auto p=dir/ManifestName,tmp=dir/(std::string(ManifestName)+".tmp");safePath(tmp);write(tmp,bytes(encode(m)));
#ifdef _WIN32
 require(MoveFileExW(tmp.c_str(),p.c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH)!=FALSE,"Manifest commit failed");
#else
 fs::rename(tmp,p);
#endif
}
struct Installer{
    fs::path release;unsigned width=1920,height=1080;std::vector<std::string> log;
    void note(const std::string& s){log.push_back(s);}
    Bytes payload(const std::string& name,const std::string& expected){auto b=read(release/"files"/name);hashIs(b,expected,name);return b;}
    std::map<std::string,Bytes> plan(const fs::path& target,const std::string& preset){
        require(preset=="D3D11"||preset=="D3D9"||preset=="D3D8","Unsupported x86 preset");
        safePath(target);require(machine(read(target))==0x14c,"Target must be PE32/x86; x64 targets are not supported");auto dir=installDirectory(target);
        std::map<std::string,Bytes> p;
        auto sums=str(read(release/"payload.sha256"));
        for(auto name:{"dlss5-neural.addon32","dlss5-neural-host64.exe"}){
            std::smatch m;require(std::regex_search(sums,m,std::regex("([0-9a-fA-F]{64})  "+std::string(name)+"(?:\\r?\\n|$)")),"Missing bridge release checksum");
            p[name]=payload(name,lower(m[1]));require(machine(p[name])==(std::string(name).find("addon32")!=std::string::npos?0x14c:0x8664),"Wrong bridge architecture");
        }
        p["dlssnr_amd_pass1.dll"]=payload("dlssnr_amd_pass1.dll",RuntimeSha);
        p["dlssnr_on_amd_weights.bin"]=payload("dlssnr_on_amd_weights.bin",WeightsSha);
        if(preset=="D3D8"){
            auto translator=payload("d3d8to9.dll",D3D8To9Sha);require(machine(translator)==0x14c,"d3d8to9 must be x86");
            std::string translatorName="d3d8.dll";auto existing=dir/translatorName;safePath(existing);
            if(fs::exists(existing)&&hashFile(existing)!=D3D8To9Sha){
                require(advertisesD3D8Sidecar(read(existing)),"Existing d3d8.dll does not advertise d3d8R.dll chaining; preserved");
                translatorName="d3d8R.dll";
            }
            p[translatorName]=std::move(translator);
        }
        const std::string reshadeName=preset=="D3D11"?"dxgi.dll":"d3d9.dll";
        if(fs::exists(release/"files/dxgi.dll"))p[reshadeName]=payload("dxgi.dll",ReShadeSha);
        else {require(fs::exists(dir/reshadeName),"Install official ReShade 6.8 Full Add-on Support for the selected API first, or provide private files/dxgi.dll");auto b=read(dir/reshadeName);hashIs(b,ReShadeSha,"Existing ReShade (requires tested 6.8.0.2156 x86 full-addon binary)");p[reshadeName]=std::move(b);}
        require(machine(p[reshadeName])==0x14c,"ReShade must be x86");
        auto tuning=dir/"dlss5-neural.ini";safePath(tuning);if(!fs::exists(tuning))p["dlss5-neural.ini"]=bytes(freshIni());
        auto ini=dir/"ReShade.ini";safePath(ini);auto before=fs::exists(ini)?str(read(ini)):std::string();auto after=firstDock(before,width,height);
        if(before!=after)p["ReShade.ini"]=bytes(after);
        return p;
    }
    void install(const fs::path& target,const std::string& preset){
        auto dir=installDirectory(target);safePath(dir);safePath(dir/ManifestName);
        auto desired=plan(fs::absolute(target),preset);Manifest m;m.preset=preset;
        if(fs::exists(dir/ManifestName)){m=decode(str(read(dir/ManifestName)));require(m.state=="installed","Interrupted transaction: run uninstall/recovery before reinstall");require(m.preset==preset,"Uninstall previous preset before changing API");}
        struct Change{std::string name;Bytes before,after;bool existed;};std::vector<Change> changes;
        const auto stamp=std::to_string(std::chrono::system_clock::now().time_since_epoch().count());
        for(auto& [name,data]:desired){
            auto dst=dir/name;safePath(dst);const bool exists=fs::exists(dst);const auto wanted=sha(data),old=exists?hashFile(dst):std::string();
            auto e=std::find_if(m.entries.begin(),m.entries.end(),[&](const Entry& x){return x.name==name;});
            if(exists&&old==wanted){note("IDENTICAL: "+name);if(e==m.entries.end())m.entries.push_back({name,wanted,"","",false,config(name)});continue;}
            if(e!=m.entries.end()&&exists&&old!=e->hash){
                if(config(name)){note("PRESERVED user-modified config: "+name);continue;}
                throw std::runtime_error("File changed since install; preserved: "+name);
            }
            const Bytes before=exists?read(dst):Bytes();
            if(e==m.entries.end()){
                Entry item{name,wanted,"","",true,config(name)};
                if(exists){item.backup=".dlss5-x86bridge-backups/"+stamp+"/"+name;item.backupHash=old;auto bp=dir/item.backup;safePath(bp);fs::create_directories(bp.parent_path());write(bp,before);hashIs(read(bp),old,"Backup");note("EXTERNAL backed up: "+name);}else note("CREATE: "+name);
                m.entries.push_back(item);
            }else {
                if(!e->owned&&exists){e->backup=".dlss5-x86bridge-backups/"+stamp+"/"+name;e->backupHash=old;auto bp=dir/e->backup;safePath(bp);fs::create_directories(bp.parent_path());write(bp,before);hashIs(read(bp),old,"Upgrade backup");}
                e->hash=wanted;e->owned=true;
            }
            changes.push_back({name,before,data,exists});
        }
        // Journal precedes target writes. Uninstall can recover interrupted installs using hashes.
        const bool hadManifest=fs::exists(dir/ManifestName);const auto oldManifest=hadManifest?read(dir/ManifestName):Bytes();
        m.state="installing";atomicManifest(dir,m);
        try{for(auto& c:changes)write(dir/c.name,c.after);m.state="installed";atomicManifest(dir,m);}
        catch(...){for(auto i=changes.rbegin();i!=changes.rend();++i){if(i->existed)write(dir/i->name,i->before);else fs::remove(dir/i->name);}if(hadManifest)write(dir/ManifestName,oldManifest);else fs::remove(dir/ManifestName);throw;}
        note("Installed "+preset+" x86; "+(preset=="D3D8"?std::string("d3d8to9 ")+D3D8To9Version+" -> native D3D9 frontend":"native frontend")+"; same-frame protocol v2");
    }
    void uninstall(const fs::path& directory,bool removeConfigs=false){
        auto dir=fs::absolute(directory);safePath(dir);safePath(dir/ManifestName);require(fs::exists(dir/ManifestName),"No x86 install manifest");auto m=decode(str(read(dir/ManifestName)));std::vector<Entry> keep;
        for(auto& e:m.entries){auto dst=dir/e.name;safePath(dst);if(!e.owned){note("PRESERVED pre-existing identical file: "+e.name);continue;}
            if(!e.backup.empty()){safePath(dir/e.backup);hashIs(read(dir/e.backup),e.backupHash,"Original backup");}
            if(fs::exists(dst)&&hashFile(dst)!=e.hash){
                if(m.state=="installing"&&!e.backup.empty()&&hashFile(dst)==e.backupHash){fs::remove(dir/e.backup);continue;}
                note("WARNING modified after install; retained with backup: "+e.name);keep.push_back(e);continue;
            }
            if(!fs::exists(dst)&&e.backup.empty())continue;
            if(e.configuration&&e.backup.empty()&&!removeConfigs){note("PRESERVED personal/default configuration: "+e.name);keep.push_back(e);continue;}
            if(!e.backup.empty()){write(dst,read(dir/e.backup));fs::remove(dir/e.backup);note("RESTORED: "+e.name);}
            else {fs::remove(dst);note("REMOVED: "+e.name);}
        }
        if(keep.empty())fs::remove(dir/ManifestName);else {m.entries=keep;m.state="installed";atomicManifest(dir,m);}
        note("Uninstall complete; retained files/backups are listed above.");
    }
};
}
