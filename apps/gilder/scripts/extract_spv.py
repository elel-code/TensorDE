"""Extract DXBC shaders from .rsrc section of wallpaper64.exe"""
import struct, os

EXE="/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe"
OUT="/home/yk/Code/Gilder/reverse-engineered/extracted/shaders_dxbc"

with open(EXE,'rb') as f:
    # Find .rsrc section
    f.seek(0x3C); pe_off=struct.unpack('<I',f.read(4))[0]
    f.seek(pe_off+6); num_sec=struct.unpack('<H',f.read(2))[0]
    f.seek(pe_off+20); opt_sz=struct.unpack('<H',f.read(2))[0]
    sec_off=pe_off+24+opt_sz
    
    rsrc_fo=None; rsrc_sz=0
    for i in range(num_sec):
        f.seek(sec_off+i*40)
        name=f.read(8).rstrip(b'\x00')
        vsize=struct.unpack('<I',f.read(4))[0]
        vaddr=struct.unpack('<I',f.read(4))[0]
        foff=struct.unpack('<I',f.read(4))[0]
        if name==b'.rsrc':
            rsrc_fo=foff; rsrc_sz=vsize
            break
    
    if not rsrc_fo:
        print("No .rsrc section found")
        exit(1)
    
    print(f".rsrc at file 0x{rsrc_fo:x}, size {rsrc_sz}")
    
    # Read the resource directory
    f.seek(rsrc_fo)
    rsrc_data=f.read(rsrc_sz)
    
    # Parse resource directory (3 levels)
    def parse_dir(data, off, level=0):
        if off+16>len(data): return []
        num_named=struct.unpack('<H',data[off+12:off+14])[0]
        num_id=struct.unpack('<H',data[off+14:off+16])[0]
        total=num_named+num_id
        entries=[]
        eoff=off+16
        for i in range(total):
            if eoff+8>len(data): break
            name_or_id=struct.unpack('<I',data[eoff:eoff+4])[0]
            entry_off=struct.unpack('<I',data[eoff+4:eoff+8])[0]
            entries.append((name_or_id, entry_off, level))
            eoff+=8
        return entries
    
    # Level 1: type directory
    types=parse_dir(rsrc_data,0)
    
    shader_count=0
    os.makedirs(OUT,exist_ok=True)
    
    for tid,t_off,_ in types:
        # Level 2: name directory
        names=parse_dir(rsrc_data,t_off&0x7FFFFFFF,1)
        for nid,n_off,_ in names:
            # Level 3: language directory
            langs=parse_dir(rsrc_data,n_off&0x7FFFFFFF,2)
            for lid,l_off,_ in langs:
                # Data entry
                d_off=l_off&0x7FFFFFFF
                if d_off+16>len(rsrc_data): continue
                data_rva=struct.unpack('<I',rsrc_data[d_off:d_off+4])[0]
                data_size=struct.unpack('<I',rsrc_data[d_off+4:d_off+8])[0]
                
                # Convert RVA to file offset
                for i in range(num_sec):
                    f.seek(sec_off+i*40+12)
                    svaddr=struct.unpack('<I',f.read(4))[0]
                    sfo=struct.unpack('<I',f.read(4))[0]
                    f.seek(sec_off+i*40+8)
                    svsize=struct.unpack('<I',f.read(4))[0]
                    if svaddr<=data_rva<svaddr+svsize:
                        file_off=sfo+(data_rva-svaddr)
                        f.seek(file_off)
                        blob=f.read(min(data_size,64))
                        # Check for DXBC magic: 'DXBC'
                        if blob[:4]==b'DXBC':
                            shader_count+=1
                            fname=f'{OUT}/shader_{tid}_{nid}_{lid}.dxbc'
                            f.seek(file_off)
                            with open(fname,'wb') as sf:
                                sf.write(f.read(data_size))
                            print(f'  DXBC shader: {fname} ({data_size} bytes)')
                        break
    
    print(f'Total DXBC shaders extracted: {shader_count}')
