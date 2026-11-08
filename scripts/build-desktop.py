from pathlib import Path
import subprocess,os,sys
r=Path(__file__).resolve().parent.parent
core=r/'dist'/('gatherforge-core.exe' if os.name=='nt' else 'gatherforge-core')
core.parent.mkdir(exist_ok=True)
subprocess.run(['go','build','-trimpath','-o',str(core),'.'],cwd=r,check=True)
subprocess.run([sys.executable,'-m','PyInstaller','--clean','--noconfirm','--onefile','--windowed','--name','GatherForge','--distpath',str(r/'dist'),'--workpath',str(r/'build'),'--specpath',str(r/'build'),'--add-binary',f'{core}{os.pathsep}.',str(r/'desktop/gatherforge.py')],cwd=r,check=True)
