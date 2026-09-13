"""Portable contract/control-flow and additive audits. Not Windows/GPU validation."""
from pathlib import Path
import hashlib,json,re,subprocess,tempfile,shutil
root=Path(__file__).resolve().parents[1]
new=root/'src/x86bridge'
original={}
for line in (root/'docs/x86bridge-baseline.sha256').read_text().splitlines():
 h,p=line.split('  ',1);original[p]=h
modified=[p for p,h in original.items() if (root/p).exists() and hashlib.sha256((root/p).read_bytes()).hexdigest()!=h]
deleted=[p for p in original if not (root/p).is_file()]
assert not modified and not deleted,(modified,deleted)
print(f'PASS preservation original_files={len(original)} modified_existing_files=0 deleted_existing_files=0')
f=(new/'frontend32.cpp').read_text();h=(new/'host64.cpp').read_text();ipc=(new/'bridge_ipc.h').read_text();io=(new/'bridge_io.h').read_text()
for word in ['d3d12','amdhip','dlssnr_amd_pass','Packet','RecordFn','ID3D11Device5','ID3D11DeviceContext4','OpenSharedFence','SetEventOnCompletion']:
 assert word.lower() not in f.lower(),word
for word in ['VORT','Generic Depth','async_home','amd_last_nr','g.sent_n','retryAfter','soft_timeout','ping-pong']:
 assert word not in f+h+ipc+io,word
assert '#include "../neural/neural.cpp"' in h and '#define DLSS5_WITH_VULKAN 0' in h
assert h.count('DllMain(')==0
assert 'EnumAdapterByLuid' in h and 'got.LowPart==luid.LowPart&&got.HighPart==luid.HighPart' in h
assert 'DuplicateHandle(GetCurrentProcess(),source.handle.value,g.process.value' in f
assert 'CreateSharedHandle(' in f and 'OpenSharedHandle(handle.value' in h
assert 'PIPE_REJECT_REMOTE_CLIENTS' in f and 'GetNamedPipeClientProcessId' in f
assert 'WaitForMultipleObjects(2,waits,FALSE,INFINITE)' in io
# All protocol fields have explicitly sized scalar or packed protocol types.
for body in re.findall(r'struct \w+\s*\{(.*?)\};',ipc,re.S):
 assert not re.search(r'\b(?:bool|size_t|HANDLE|uintptr_t|intptr_t|long|double)\b|\*|std::string',body),body
# The normal return never references the shared output before same-frame confirmation.
present=f[f.index('void OnPresent('):f.index('\n}\nextern "C"')]
assert present.index('CopyResource(g.stageIn11')<present.index('CopyResource(g.colour.on11')<present.index('FlushAndWait11()')<present.index('Kind::Frame,&f')<present.index('Confirmed(a,f,g.transport)')<present.index('CopyResource(g.stageOut11')<present.index('CopyResource(bb.Get(),g.stageOut11')
assert 'mods&g.toggleMods' in present and 'IsIconic' in present and 'g.reset=true' in present
assert 'g.game11ctx->ClearState()' in f
assert 'const std::wstring name=L"\\\\\\\\.\\\\pipe\\\\dlss5-x86bridge-"' in f
# Verify the copied job-pending decision has identical executable text to upstream.
u=(root/'src/neural/neural.cpp').read_text(encoding='utf-8-sig')
a=u[u.index('    bool runNetwork = true;',u.index('void BridgePresent')):u.index('    // 1. the game',u.index('void BridgePresent'))]
b=h[h.index('    bool runNetwork = true;'):h.index('    const UINT i =',h.index('    bool runNetwork = true;'))]
normal=lambda s:re.sub(r'\s+','',re.sub(r'//[^\n]*','',s))
assert normal(a)==normal(b),'job pending policy differs'
# Guide decisions and conversion kernel are executable copies of upstream, not new heuristics.
def function(text,name):
 clean=re.sub(r'//[^\n]*','',text)
 pos=clean.index(name+'(');start=clean.index('{',pos);depth=1;end=start+1
 while depth:
  depth+=(clean[end]=='{')-(clean[end]=='}');end+=1
 return normal(clean[pos:end])
for fn in ['GuideDepthSrvFormat','LooksLikeMotion','SettleGuide']:
 assert function(u,fn)==function(f,fn),fn
assert re.search(r'constexpr char kGuideDepthCs\[\] = R"\((.*?)\)";',u,re.S)[1]==re.search(r'constexpr char kGuideDepthCs\[\] = R"\((.*?)\)";',f,re.S)[1]

transport=h[h.index('    Result CopyOnly()'):h.index('    Result Neural()')]
for call in ['InitHip(','InitEngine(','BringUpEngines(','RecordNetwork(','LoadLibrary','RuntimeHashMatches(']:assert call not in transport
assert 'cmd->CopyResource(g.crossLocal.Get(),g.bridgeIn.on12.Get())' in transport
assert 'cmd->CopyResource(g.bridgeOut.on12.Get(),g.crossLocal.Get())' in transport
assert 'Idle();return Result::Transport' in transport
neural=h[h.index('    Result Neural()'):h.index('    Result FrameWork(')]
assert neural.index('RecordNetwork(')<neural.index('CompositionIsFresh(')<neural.index('cmd->Close()')<neural.index('ExecuteCommandLists(')<neural.index('NotifyFn')<neural.index('WaitForWorkQueue(g.completion)')<neural.index('return fresh?')
assert 'runNetwork&&g.activePasses!=0' in neural and 'g.fence->GetCompletedValue()>=g.completion' in neural
assert h.count('++g.frame')==1 and 'Idle();ReleaseSwapchainSized();built=false;g.historyValid.store(false);' in h
for path in [new/'frontend32.cpp',new/'host64.cpp']:
 assert 'ProfileForThisProcess' not in path.read_text() and 'kTargets' not in path.read_text()
 assert set(re.findall(r'[A-Za-z0-9_-]+\.exe',path.read_text())) <= {'dlss5-neural-host64.exe'}
print('PASS static boundaries: original engine TU; no neural imports/API in frontend; generic LUID match; fixed-width IPC; shared handle ownership; same-frame confirmation; host output fence before ACK; transport isolated; original pending policy; no cached output')
compiler=shutil.which('g++')
if not compiler:raise SystemExit('Portable compiler missing; native MSVC checks are separate')
with tempfile.TemporaryDirectory(prefix='x86bridge-tests-') as d:
 p=Path(d)
 subprocess.run([compiler,'-std=c++20','-Wall','-Wextra','-Werror',str(new/'protocol_test.cpp'),'-o',str(p/'protocol')],check=True)
 subprocess.run([str(p/'protocol')],check=True)
 # Test real transfer helper using Win32 doubles, including short reads and peer death.
 (p/'windows.h').write_text(r'''
#pragma once
#include <cstdint>
#include <algorithm>
#include <cassert>
#include <cstring>
using HANDLE=void*;using DWORD=uint32_t;using BOOL=int;
constexpr BOOL FALSE=0,TRUE=1;const HANDLE INVALID_HANDLE_VALUE=reinterpret_cast<HANDLE>(-1);
constexpr DWORD INFINITE=0xffffffff,ERROR_IO_PENDING=997,WAIT_OBJECT_0=0;
struct OVERLAPPED{HANDLE hEvent;};
inline int mode=0,calls=0,cancelled=0,retired=0,closed=0,waited=0;inline DWORD amount=0;inline void* target=nullptr;
inline BOOL CloseHandle(HANDLE){++closed;return 1;}
inline HANDLE CreateEventW(void*,BOOL,BOOL,void*){return reinterpret_cast<HANDLE>(1);}
inline BOOL ReadFile(HANDLE,void* b,DWORD n,DWORD* done,OVERLAPPED*){++calls;amount=std::min(n,DWORD(3));target=b;if(mode==1||mode==2)return 0;*done=mode==3?0:amount;memset(b,42,*done);return 1;}
inline BOOL WriteFile(HANDLE h,void* b,DWORD n,DWORD* done,OVERLAPPED* ov){return ReadFile(h,b,n,done,ov);}
inline DWORD GetLastError(){return mode==4?5:ERROR_IO_PENDING;}
inline DWORD WaitForMultipleObjects(DWORD n,HANDLE*,BOOL,DWORD timeout){assert(n==2&&timeout==INFINITE);++waited;return mode==2?1:0;}
inline BOOL CancelIoEx(HANDLE,OVERLAPPED*){++cancelled;return 1;}
inline BOOL GetOverlappedResult(HANDLE,OVERLAPPED*,DWORD* n,BOOL wait){if(wait){++retired;return 0;}*n=amount;memset(target,42,amount);return 1;}
''')
 (p/'io.cpp').write_text(r'''
#include "bridge_io.h"
#include <cstdio>
int main(){char b[19]{};HANDLE pipe=reinterpret_cast<HANDLE>(2),peer=reinterpret_cast<HANDLE>(3);
 assert(x86bridge::Receive(pipe,peer,b,sizeof(b)));assert(calls==7&&!waited&&b[18]==42);
 mode=1;calls=0;assert(x86bridge::Receive(pipe,peer,b,sizeof(b)));assert(calls==7&&waited==7);
 mode=2;assert(!x86bridge::Receive(pipe,peer,b,sizeof(b)));assert(cancelled==1&&retired==1);
 mode=3;assert(!x86bridge::Receive(pipe,peer,b,sizeof(b)));
 printf("PASS actual IPC helper CPU flow: partial immediate/deferred receive; peer death cancels and retires I/O before stack lifetime ends; zero-byte reply rejected. Win32/GPU UNVALIDATED.\n");
}
''')
 subprocess.run([compiler,'-std=c++20','-Wall','-Wextra','-Werror','-Wno-misleading-indentation','-I'+str(p),'-I'+str(new),str(p/'io.cpp'),'-o',str(p/'io')],check=True)
 subprocess.run([str(p/'io')],check=True)
print('UNVALIDATED: MSVC x86/x64 compilation and PE imports, original addon64 native build, real ReShade/GPU/HIP/resize/crash tests. No native binaries produced by these tests.')

subprocess.run([__import__('sys').executable,str(root/'tools/test-x86bridge-v2.py')],check=True)

subprocess.run([__import__('sys').executable,str(root/'tools/test-x86bridge-factory.py')],check=True)
