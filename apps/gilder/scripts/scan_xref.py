"""Find all reads from struct offset 0x5b8 (composelayer_clearalpha consumer)"""
import struct
EXE="/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe"
with open(EXE,'rb') as f:
    f.seek(0x400); d=f.read(0x42490C)

# Search for any instruction pattern that reads from [reg+0x5b8]
# 48 8B 8X B8 05 00 00 = mov r64, [reg+0x5b8]
sites=[]
for i in range(len(d)-7):
    if d[i]==0x48 and d[i+1] in (0x8B,0x8D):
        modrm=d[i+2]
        if d[i+3:i+7]==b'\xb8\x05\x00\x00':
            regs=['rax','rcx','rdx','rbx','rsp','rbp','rsi','rdi']
            reg=regs[modrm&7] if (modrm&0xC0)!=0xC0 else '?'
            addr=0x140001000+i
            if addr>0x140100000:  # runtime only
                sites.append((addr,reg))
print(f"Reads from +0x5b8 (runtime, >0x140100000): {len(sites)}")
for addr,reg in sites[:30]:
    print(f"  0x{addr:x} [{reg}+0x5b8]")
