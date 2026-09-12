from pathlib import Path
import re,subprocess,tempfile,shutil
r=Path(__file__).resolve().parents[1];n=r/'src/x86bridge'
s=(n/'overlay32.inc').read_text()
pre='''#include <imgui.h>
#include "control_state.h"
#include <string>
#include <mutex>
#include <cstdio>
#include <cstring>
#include <algorithm>
using UINT=unsigned;using LONG=int;
struct effect_runtime{};
'''
pre+='\n'.join(f'constexpr int {k}={i+1};' for i,k in enumerate(sorted(set(re.findall(r'\b(?:VK_\w+|MAPVK_VK_TO_VSC|CP_UTF8)\b',s)))))+'\n'
pre+='''inline unsigned MapVirtualKeyW(unsigned,unsigned){return 0;}
inline int GetKeyNameTextW(LONG,wchar_t*,int){return 0;}
inline int WideCharToMultiByte(unsigned,unsigned,const wchar_t*,int,char*,int,void*,void*){return 0;}
inline short GetAsyncKeyState(int){return 0;}
inline uint64_t GetTickCount64(){return 0;}
struct Ptr{void* Get()const{return nullptr;}};
struct Guide{Ptr chosen;unsigned width=0,height=0,format=0;bool ready=false;};
struct Front{std::mutex lock;Guide guideDepth,guideMotion;bool failed=false;}g;
struct Controls{x86bridge::WireSettings shadow;x86bridge::WireStatus status;uint64_t overlayAt=0;bool synced=false,syncRequested=false,save=false,reload=false,factory=false,measure=false,capturing=false;}controls;
void OperationalSettings(){}
#include "overlay32.inc"
'''
with tempfile.TemporaryDirectory(prefix='x86bridge-ui-') as d:
 p=Path(d)/'overlay.cpp';p.write_text(pre)
 subprocess.run([shutil.which('g++'),'-std=c++20','-Wall','-Wextra','-Werror','-fsyntax-only','-I'+str(n),'-I'+str(r/'external/reshade'),str(p)],check=True)
print('PASS overlay source syntax against real bundled imgui.h; Win32 functions mocked, not native ABI validation')

# Compile the actual host ExportSettings/ApplySettings bodies, with only original g atomics doubled.
h=(n/'host64.cpp').read_text()
methods=h[h.index('    WireSettings ExportSettings()'):h.index('    WireStatus ExportStatus()')]
state=r'''#include "control_state.h"
#include <atomic>
#include <cassert>
#include <limits>
#include <cstring>
using namespace x86bridge;
struct Engine {
#define X(type,name,low,high) std::atomic<type> name{};
#include "settings_fields.inc"
#undef X
 std::atomic<bool> historyValid{true},passOverride[3]{};
 std::atomic<float> passStructure[3]{},passTone[3]{},passSkin[3]{};
}g;
struct Host{uint64_t settingsRevision=1;
'''+methods+r'''};
int main(){Host host;WireSettings s; s.settings_revision=2;s.skin=-1;s.useHistory=1;s.scale=.5f;s.structure=1;s.tone=1;
 g.useHistory.store(1);assert(host.ApplySettings(s));assert(g.historyValid.load());assert(g.skin.load()==-1);assert(g.inlineMode.load()==1);
 auto before=host.ExportSettings();assert(!host.ApplySettings(s));auto after=host.ExportSettings();assert(std::memcmp(&before,&after,sizeof(before))==0);
 s.settings_revision=3;s.scale=std::numeric_limits<float>::infinity();assert(!host.ApplySettings(s));after=host.ExportSettings();assert(std::memcmp(&before,&after,sizeof(before))==0);
 s.scale=.5f;s.useHistory=0;assert(host.ApplySettings(s));assert(!g.historyValid.load());
 g.historyValid.store(true);s.settings_revision=4;s.structure=2;assert(host.ApplySettings(s));assert(g.historyValid.load());
 s.settings_revision=5;s.startOn=1;s.toggleKey=65;s.toggleMods=5;s.language=1;s.disableOnAltTab=1;assert(host.ApplySettings(s));
 after=host.ExportSettings();assert(after.startOn==1&&after.toggleKey==65&&after.toggleMods==5&&after.language==1&&after.disableOnAltTab==1);
}
'''
with tempfile.TemporaryDirectory(prefix='x86bridge-state-') as d:
 p=Path(d);(p/'state.cpp').write_text(state)
 subprocess.run([shutil.which('g++'),'-std=c++20','-Wall','-Wextra','-Werror','-I'+str(n),str(p/'state.cpp'),'-o',str(p/'state')],check=True)
 subprocess.run([str(p/'state')],check=True)
print('PASS actual host Apply/Export: finite clamp, stale reject/no mutation, history change only, operational fields mirrored, forced inline')
# No I/O in UI or its local helpers. The only control transaction owner is OnPresent.
ui=(n/'overlay32.inc').read_text();front=(n/'frontend32.cpp').read_text()
for call in ['Request(', 'ReadFile(', 'WriteFile(', 'WaitFor', 'FlushAndWait', 'StartHost(', 'StateRequest(', 'SyncControls(', 'SaveSettings(', 'LoadSettings(']:
 assert call not in ui,call
assert 'std::try_to_lock' in ui and 'controls.save=true' in ui and 'controls.reload=true' in ui and 'controls.measure=true' in ui
assert front.count('SyncControls()')==2 # definition + OnPresent call
assert front.index('if(!SyncControls()')<front.index('Kind::Frame,&f')
for name in ['Silent Hill','Resident Evil','God of War','GTA V','NFS','ProfileForThisProcess','kTargets']:
 assert name not in ui+front+h,name
assert 'Async previous-frame presentation is not implemented' in ui
assert 'g.inlineMode.store(true)' in h and 'LoadSettings();ForceInline();' in h
assert 'register_overlay("DLSS Neural Rendering (AMD)",OnOverlay32)' in front
assert h.index('SaveSettings();Snapshot')>h.index('case Kind::SaveSettings:')
fields=(n/'settings_fields.inc').read_text()
assert all(re.fullmatch(r'X\((uint32_t|int32_t|float), [A-Za-z]+, [-.0-9]+, [-.0-9]+\)',line) for line in fields.splitlines() if not line.startswith('//'))
print('PASS overlay has no IPC/waits; present-only controls; frontend-only shadow; original host Save/Reload; same-frame lock; generic source')
