#include "platform.h"
#include <iostream>
#include <cassert>
namespace i=install86;namespace fs=std::filesystem;
int passed=0;
void check(bool b,const char* msg){i::require(b,msg);++passed;std::cout<<"PASS "<<msg<<"\n";}
template<class F> void rejects(F f,const char* msg){bool threw=false;try{f();}catch(const std::exception&){threw=true;}check(threw,msg);}
i::Bytes pe(bool x64=false){i::Bytes b(512);b[0]='M';b[1]='Z';b[60]=128;b[128]='P';b[129]='E';b[132]=x64?0x64:0x4c;b[133]=x64?0x86:1;b[152]=x64?0x0b:0x0b;b[153]=x64?2:1;return b;}
int main(int argc,char** argv){try{
    i::require(argc<=2,"usage: installer-tests.exe [release fixture folder]");
    auto root=fs::temp_directory_path()/("x86-installer-test-"+std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()));fs::create_directory(root);
    struct Clean{fs::path p;~Clean(){std::error_code ec;fs::remove_all(p,ec);}}clean{root};
    check(i::sha(i::bytes("abc"))=="ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","SHA256 known vector");
    for(auto size:{std::pair<unsigned,unsigned>{1280,720},{2560,1440}}){auto s=i::firstDock("[INPUT]\nKeyOverlay=36,0,0,0\n",size.first,size.second);
        check(s.find("Size="+std::to_string(size.first)+",,"+std::to_string(size.second))!=s.npos,"fresh docking follows supplied viewport");
        check(i::getIni(s,"INPUT","KeyOverlay")=="36,0,0,0","docking preserves other INI sections");
        check(i::firstDock(s,800,600)==s,"saved panel layout never redocked");
    }
    std::string custom="[OVERLAY]\nDocking=other-layout\nWindow=[Window][###home],DockId=0x12345678,,1,[Window][Other],Pos=10,,20\n[STYLE]\nAlpha=0.7\n";
    auto merge=i::firstDock(custom,1920,1080);
    check(merge.find("[Window][DLSS Neural Rendering (AMD)],Collapsed=0,DockId=0x12345678")!=merge.npos,"existing Home docking ID reused");
    check(i::getIni(merge,"OVERLAY","Docking")=="other-layout"&&i::getIni(merge,"STYLE","Alpha")=="0.7","existing docking geometry and style untouched");
    std::string undocked="[OVERLAY]\nWindow=[Window][DLSS Neural Rendering (AMD)],Pos=37,,49,Size=600,,400\n";
    check(i::firstDock(undocked,1920,1080)==undocked,"user-undocked window respected");
    i::Manifest manifest;manifest.preset="D3D11";manifest.entries.push_back({"dlss5-neural.addon32",std::string(64,'a'),"","",true,false});
    check(i::encode(i::decode(i::encode(manifest)))==i::encode(manifest),"manifest canonical roundtrip");
    rejects([&]{i::decode(i::encode(manifest)+"junk");},"modified manifest rejected");
    if(argc!=2){std::cout<<"TOTAL PASS="<<passed<<". Core installer tests; pinned payload fixture not supplied.\n";return 0;}
    fs::path release=fs::absolute(argv[1]);i::Installer app;app.release=release;app.extract=i::extractArchive;
    for(auto preset:{"D3D11","D3D9","D3D8"}){
        auto dir=root/preset;fs::create_directory(dir);auto target=dir/"target.exe";i::write(target,pe());app.install(target,preset);
        check(i::hashFile(dir/"dxgi.dll")==i::ReShadeSha,"ReShade x86 installed only as DXGI");
        check(i::getIni(i::str(i::read(dir/"dlss5-neural.ini")),"dlss5","ColourStrength")=="0.25","fresh ColourStrength=0.25");
        if(std::string(preset)=="D3D11")check(!fs::exists(dir/"d3d8.dll")&&!fs::exists(dir/"d3d9.dll")&&!fs::exists(dir/"dgVoodoo.conf"),"D3D11 receives no dgVoodoo");
        else {const bool d8=std::string(preset)=="D3D8";check(i::hashFile(dir/(d8?"d3d8.dll":"d3d9.dll"))==(d8?i::D8Sha:i::D9Sha),"wrapper is exact pinned MS/x86 entry");
            auto conf=i::str(i::read(dir/"dgVoodoo.conf"));check(i::getIni(conf,"DirectX","VideoCard")=="internal3D"&&i::getIni(conf,"DirectX","VRAM")=="4096","dgVoodoo internal3D + VRAM4096");
            check(i::getIni(conf,"DirectX","dgVoodooWatermark")=="false"&&i::getIni(conf,"DirectX","FastVideoMemoryAccess")=="false","watermark and fast VRAM access disabled");
            check(i::getIni(conf,"General","OutputAPI")=="d3d11_fl11_0","wrapper output forced to D3D11");
        }
        auto before=i::read(dir/i::ManifestName);app.install(target,preset);check(i::read(dir/i::ManifestName)==before,"reinstall manifest idempotent");
        i::write(dir/"dlss5-neural.ini",i::bytes("[dlss5]\nColourStrength=0.65\nScale=0.75\n"));auto tuning=i::read(dir/"dlss5-neural.ini");app.install(target,preset);check(i::read(dir/"dlss5-neural.ini")==tuning,"reinstall preserves user tuning byte-for-byte");
        app.uninstall(dir);check(!fs::exists(dir/"dxgi.dll")&&!fs::exists(dir/"dlss5-neural.addon32")&&!fs::exists(dir/"dlss5-neural-host64.exe"),"uninstall removes owned bridge binaries");
        check(i::read(dir/"dlss5-neural.ini")==tuning,"uninstall retains modified personal config");
        check(!fs::exists(dir/"d3d8.dll")&&!fs::exists(dir/"d3d9.dll"),"uninstall leaves no owned wrapper DLL");
    }
    auto conflict=root/"conflict";fs::create_directory(conflict);auto target=conflict/"target.exe";i::write(target,pe());
    std::map<std::string,i::Bytes> originals;
    for(auto name:{"dxgi.dll","d3d9.dll","dgVoodoo.conf","ReShade.ini","dlss5-neural.ini"}){originals[name]=i::bytes(std::string(name).find(".dll")!=std::string::npos?"external DLL":std::string(name)=="dlss5-neural.ini"?"[dlss5]\nColourStrength=0.8\n":"[USER]\nPreserve=yes\n");i::write(conflict/name,originals[name]);}
    app.install(target,"D3D9");check(i::read(conflict/"dlss5-neural.ini")==originals["dlss5-neural.ini"],"pre-existing tuning never recreated");app.uninstall(conflict);
    for(auto& [name,b]:originals)check(i::read(conflict/name)==b,"conflicting DLL/config backup restored exactly");
    auto changed=root/"changed";fs::create_directory(changed);i::write(changed/"target.exe",pe());app.install(changed/"target.exe","D3D11");i::write(changed/"dxgi.dll",i::bytes("user replacement"));app.uninstall(changed);check(i::str(i::read(changed/"dxgi.dll"))=="user replacement","uninstall preserves DLL replaced after install");
    i::write(root/"x64.exe",pe(true));rejects([&]{app.install(root/"x64.exe","D3D11");},"x64 target refused");
    rejects([&]{app.install(target,"D3D12");},"unsupported API refused");
    auto corrupt=root/"corrupt";fs::create_directory(corrupt);fs::create_directory(corrupt/"files");
    for(auto& f:fs::directory_iterator(release/"files"))fs::copy_file(f.path(),corrupt/"files"/f.path().filename());fs::copy_file(release/"payload.sha256",corrupt/"payload.sha256");
    i::write(corrupt/"dgVoodoo2_87_4.zip",i::bytes("corrupt"));i::Installer bad=app;bad.release=corrupt;
    rejects([&]{bad.plan(target,"D3D9");},"corrupt wrapper ZIP refused before mutation");
    i::write(corrupt/"files/dlssnr_amd_pass1.dll",i::bytes("wrong runtime"));rejects([&]{bad.plan(target,"D3D11");},"wrong runtime rejected");
    fs::copy_file(release/"files/dlssnr_amd_pass1.dll",corrupt/"files/dlssnr_amd_pass1.dll",fs::copy_options::overwrite_existing);i::write(corrupt/"files/dlssnr_on_amd_weights.bin",i::bytes("wrong weights"));rejects([&]{bad.plan(target,"D3D11");},"wrong weights rejected");
    std::cout<<"TOTAL PASS="<<passed<<". Filesystem/INI/payload tests; Windows GUI/ReShade live docking/GPU UNVALIDATED.\n";return 0;
}catch(const std::exception& e){std::cerr<<"FAIL "<<e.what()<<"\n";return 1;}}
