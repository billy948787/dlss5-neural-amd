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
    check(i::advertisesD3D8Sidecar(i::bytes("prefix D3D8R.DLL suffix")),"ASCII d3d8 sidecar marker detected");
    i::Bytes wideMarker;for(char c:std::string("d3d8R.dll")){wideMarker.push_back(c);wideMarker.push_back(0);}check(i::advertisesD3D8Sidecar(wideMarker),"UTF-16 d3d8 sidecar marker detected");
    check(!i::advertisesD3D8Sidecar(i::bytes("ordinary d3d8.dll wrapper")),"unknown d3d8 wrapper is not assumed chainable");
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
    i::Manifest legacy;legacy.preset="D3D9";legacy.entries=manifest.entries;auto legacyText=i::encode(legacy);
    const std::string nativeMarker="\"dgVoodoo\":\"none\"";auto marker=legacyText.find(nativeMarker);i::require(marker!=legacyText.npos,"legacy marker");
    legacyText.replace(marker,nativeMarker.size(),"\"dgVoodoo\":\"2.87.4\"");
    check(i::decode(legacyText).preset=="D3D9","legacy D3D9 manifest remains uninstallable");
    legacy.preset="D3D8";legacyText=i::encode(legacy);marker=legacyText.find(nativeMarker);i::require(marker!=legacyText.npos,"legacy D3D8 marker");
    legacyText.replace(marker,nativeMarker.size(),"\"dgVoodoo\":\"2.87.4\"");
    check(i::decode(legacyText).preset=="D3D8","legacy D3D8 manifest remains uninstallable");
    auto redirected=root/"redirected";fs::create_directories(redirected/"bin");auto redirectedExe=redirected/"game.exe";i::write(redirectedExe,pe());
    i::write(redirected/"ReShade.ini",i::bytes("[INSTALL]\nBasePath=bin\n"));
    check(i::installDirectory(redirectedExe)==fs::weakly_canonical(redirected/"bin"),"ReShade BasePath child directory honored");
    i::write(redirected/"ReShade.ini",i::bytes("[INSTALL]\nBasePath=..\n"));
    rejects([&]{i::installDirectory(redirectedExe);},"ReShade BasePath escape rejected");
    if(argc!=2){std::cout<<"TOTAL PASS="<<passed<<". Core installer tests; pinned payload fixture not supplied.\n";return 0;}
    fs::path release=fs::absolute(argv[1]);i::Installer app;app.release=release;
    const bool hasD3D8=fs::exists(release/"files/d3d8to9.dll");
    std::vector<std::string> presets={"D3D11","D3D9"};if(hasD3D8)presets.push_back("D3D8");
    for(const auto& preset:presets){
        auto dir=root/preset;fs::create_directory(dir);auto target=dir/"target.exe";i::write(target,pe());app.install(target,preset);
        const bool d3d11=preset=="D3D11",d3d8=preset=="D3D8";const auto reshade=d3d11?"dxgi.dll":"d3d9.dll";
        check(i::hashFile(dir/reshade)==i::ReShadeSha,"ReShade x86 installed for the selected API route");
        check(i::getIni(i::str(i::read(dir/"dlss5-neural.ini")),"dlss5","ColourStrength")=="0.25","fresh ColourStrength=0.25");
        if(d3d8){check(i::hashFile(dir/"d3d8.dll")==i::D3D8To9Sha,"D3D8 route installs the pinned d3d8to9 layer");check(!fs::exists(dir/"dgVoodoo.conf"),"D3D8 route receives no dgVoodoo configuration");}
        else check(!fs::exists(dir/"d3d8.dll")&&!fs::exists(dir/"dgVoodoo.conf"),"native route receives no translation wrapper");
        if(!d3d11)check(!fs::exists(dir/"dxgi.dll"),"D3D9-based route uses the D3D9 ReShade proxy");
        else check(!fs::exists(dir/"d3d9.dll"),"native D3D11 uses the DXGI ReShade proxy");
        auto before=i::read(dir/i::ManifestName);app.install(target,preset);check(i::read(dir/i::ManifestName)==before,"reinstall manifest idempotent");
        i::write(dir/"dlss5-neural.ini",i::bytes("[dlss5]\nColourStrength=0.65\nScale=0.75\n"));auto tuning=i::read(dir/"dlss5-neural.ini");app.install(target,preset);check(i::read(dir/"dlss5-neural.ini")==tuning,"reinstall preserves user tuning byte-for-byte");
        app.uninstall(dir);check(!fs::exists(dir/"dxgi.dll")&&!fs::exists(dir/"dlss5-neural.addon32")&&!fs::exists(dir/"dlss5-neural-host64.exe"),"uninstall removes owned bridge binaries");
        check(i::read(dir/"dlss5-neural.ini")==tuning,"uninstall retains modified personal config");
        check(!fs::exists(dir/"d3d8.dll")&&!fs::exists(dir/"d3d8R.dll")&&!fs::exists(dir/"d3d9.dll"),"uninstall removes owned API DLLs");
    }
    if(hasD3D8){
        auto dir=root/"D3D8-chain";fs::create_directory(dir);auto target=dir/"target.exe";i::write(target,pe());auto wrapper=pe();auto marker=i::bytes("d3d8R.dll");wrapper.insert(wrapper.end(),marker.begin(),marker.end());i::write(dir/"d3d8.dll",wrapper);
        app.install(target,"D3D8");check(i::read(dir/"d3d8.dll")==wrapper,"chainable pre-existing D3D8 wrapper preserved");check(i::hashFile(dir/"d3d8R.dll")==i::D3D8To9Sha,"translator installed under advertised d3d8R sidecar name");
        app.uninstall(dir);check(i::read(dir/"d3d8.dll")==wrapper&&!fs::exists(dir/"d3d8R.dll"),"chained D3D8 uninstall preserves original wrapper");
    }
    auto conflict=root/"conflict";fs::create_directory(conflict);auto target=conflict/"target.exe";i::write(target,pe());
    std::map<std::string,i::Bytes> originals;
    for(auto name:{"dxgi.dll","d3d9.dll","dgVoodoo.conf","ReShade.ini","dlss5-neural.ini"}){originals[name]=i::bytes(std::string(name).find(".dll")!=std::string::npos?"external DLL":std::string(name)=="dlss5-neural.ini"?"[dlss5]\nColourStrength=0.8\n":"[USER]\nPreserve=yes\n");i::write(conflict/name,originals[name]);}
    app.install(target,"D3D9");check(i::read(conflict/"dlss5-neural.ini")==originals["dlss5-neural.ini"],"pre-existing tuning never recreated");app.uninstall(conflict);
    for(auto& [name,b]:originals)check(i::read(conflict/name)==b,"conflicting DLL/config backup restored exactly");
    auto changed=root/"changed";fs::create_directory(changed);i::write(changed/"target.exe",pe());app.install(changed/"target.exe","D3D11");i::write(changed/"dxgi.dll",i::bytes("user replacement"));app.uninstall(changed);check(i::str(i::read(changed/"dxgi.dll"))=="user replacement","uninstall preserves DLL replaced after install");
    i::write(root/"x64.exe",pe(true));rejects([&]{app.install(root/"x64.exe","D3D11");},"x64 target refused");
    if(!hasD3D8)rejects([&]{app.plan(target,"D3D8");},"D3D8 preset fails closed without the pinned sidecar");
    rejects([&]{app.install(target,"D3D12");},"unsupported API refused");
    auto corrupt=root/"corrupt";fs::create_directory(corrupt);fs::create_directory(corrupt/"files");
    for(auto& f:fs::directory_iterator(release/"files"))fs::copy_file(f.path(),corrupt/"files"/f.path().filename());fs::copy_file(release/"payload.sha256",corrupt/"payload.sha256");
    i::Installer bad=app;bad.release=corrupt;i::write(corrupt/"files/dxgi.dll",i::bytes("corrupt"));
    rejects([&]{bad.plan(target,"D3D9");},"corrupt ReShade payload refused before mutation");
    fs::copy_file(release/"files/dxgi.dll",corrupt/"files/dxgi.dll",fs::copy_options::overwrite_existing);
    i::write(corrupt/"files/dlssnr_amd_pass1.dll",i::bytes("wrong runtime"));rejects([&]{bad.plan(target,"D3D11");},"wrong runtime rejected");
    fs::copy_file(release/"files/dlssnr_amd_pass1.dll",corrupt/"files/dlssnr_amd_pass1.dll",fs::copy_options::overwrite_existing);i::write(corrupt/"files/dlssnr_on_amd_weights.bin",i::bytes("wrong weights"));rejects([&]{bad.plan(target,"D3D11");},"wrong weights rejected");
    if(hasD3D8){fs::copy_file(release/"files/dlssnr_on_amd_weights.bin",corrupt/"files/dlssnr_on_amd_weights.bin",fs::copy_options::overwrite_existing);i::write(corrupt/"files/d3d8to9.dll",i::bytes("wrong translator"));rejects([&]{bad.plan(target,"D3D8");},"wrong d3d8to9 sidecar rejected");}
    std::cout<<"TOTAL PASS="<<passed<<". Filesystem/INI/payload tests; Windows GUI/ReShade live docking/GPU UNVALIDATED.\n";return 0;
}catch(const std::exception& e){std::cerr<<"FAIL "<<e.what()<<"\n";return 1;}}
