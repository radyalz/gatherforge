#!/usr/bin/env python3
"""Native GatherForge desktop UI. All processing uses a Go subprocess, never HTTP."""
import json, subprocess, threading, tkinter as tk, sys
from tkinter import ttk, filedialog, messagebox, simpledialog
from pathlib import Path
APP=Path(__file__).resolve().parent.parent
CORE_NAME='gatherforge-core.exe' if sys.platform=='win32' else 'gatherforge-core'
CORE=(Path(getattr(sys,'_MEIPASS',APP))/CORE_NAME) if getattr(sys,'frozen',False) else (APP/'dist'/CORE_NAME)
class UI(tk.Tk):
 def __init__(self):
  super().__init__();self.title('GatherForge Desktop');self.geometry('1170x720');self.minsize(780,500)
  self.path=tk.StringVar(value=str(Path.home()/'Downloads'/'Music'));self.query=tk.StringVar();self.tracks=[];self.busy=False
  top=ttk.Frame(self,padding=12);top.pack(fill='x')
  ttk.Label(top,text='Music library:').pack(side='left');ttk.Entry(top,textvariable=self.path).pack(side='left',fill='x',expand=True,padx=8)
  ttk.Button(top,text='Choose folder',command=self.choose).pack(side='left')
  self.tabs=ttk.Notebook(self);self.tabs.pack(fill='both',expand=True,padx=12,pady=5)
  d=ttk.Frame(self.tabs,padding=12);l=ttk.Frame(self.tabs,padding=12);self.tabs.add(d,text='Downloads');self.tabs.add(l,text='Music Library')
  ttk.Label(d,text='Track URLs or playlist URLs (one per line):').pack(anchor='w')
  self.urls=tk.Text(d,height=6);self.urls.pack(fill='x',pady=8)
  bar=ttk.Frame(d);bar.pack(fill='x');ttk.Button(bar,text='Download best available audio',command=self.download).pack(side='left');ttk.Button(bar,text='Install yt-dlp',command=lambda:self.job(['--install'])).pack(side='left',padx=8)
  self.logs=tk.Text(d,height=19,state='disabled');self.logs.pack(fill='both',expand=True,pady=10)
  actions=ttk.Frame(l);actions.pack(fill='x')
  for label,act in [('Scan','scan'),('Stage','stage'),('Excel reports','export'),('Publish','publish'),('Restore staging','restore'),('Finalize','finalize'),('New batch','newbatch')]:
   ttk.Button(actions,text=label,command=lambda a=act:self.action(a)).pack(side='left',padx=2)
  controls=ttk.Frame(l);controls.pack(fill='x',pady=8)
  ttk.Label(controls,text='Search:').pack(side='left');ttk.Entry(controls,textvariable=self.query).pack(side='left',fill='x',expand=True,padx=5)
  ttk.Button(controls,text='Edit artist',command=self.edit_artist).pack(side='left',padx=3)
  ttk.Button(controls,text='Keep / Skip',command=self.toggle).pack(side='left')
  ttk.Button(controls,text='Rename artist group',command=self.rename_group).pack(side='left',padx=3)
  self.table=ttk.Treeview(l,columns=('artist','title','format','action','original'),show='headings',selectmode='browse')
  for col,w in [('artist',160),('title',240),('format',80),('action',110),('original',450)]:self.table.heading(col,text=col.title());self.table.column(col,width=w,minwidth=70)
  sb=ttk.Scrollbar(l,orient='vertical',command=self.table.yview);self.table.configure(yscrollcommand=sb.set);self.table.pack(side='left',fill='both',expand=True);sb.pack(side='right',fill='y')
  self.query.trace_add('write',lambda *_:self.render());self.status=tk.StringVar(value='Ready');ttk.Label(self,textvariable=self.status,padding=8).pack(fill='x')
  Path(self.path.get()).mkdir(parents=True,exist_ok=True);self.refresh()
 def call(self,args,input=None):
  return subprocess.run([str(CORE),'--root',self.path.get(),*args],input=input,text=True,capture_output=True,check=True)
 def choose(self):
  d=filedialog.askdirectory(initialdir=self.path.get());
  if d:self.path.set(d);self.refresh()
 def job(self,args,done=None,input=None):
  if self.busy:return
  self.busy=True;self.status.set('Processing...');self.log('> '+ ' '.join(args))
  def run():
   try:p=self.call(args,input);self.after(0,lambda:self.finish(p.stdout,done))
   except Exception as e:
    msg=e.stderr if isinstance(e,subprocess.CalledProcessError) else str(e)
    self.after(0,lambda m=msg:self.fail(m))
  threading.Thread(target=run,daemon=True).start()
 def finish(self,out,done):
  self.busy=False;self.log(out.strip() or 'Done');self.status.set('Completed');
  if done:done()
 def fail(self,msg):self.busy=False;self.status.set('Failed');self.log('ERROR: '+msg);messagebox.showerror('GatherForge',msg[-1200:])
 def log(self,msg):
  self.logs.configure(state='normal');self.logs.insert('end',msg+'\n');self.logs.see('end');self.logs.configure(state='disabled')
 def refresh(self):
  try:
   s=json.loads(self.call(['--status']).stdout);self.tracks=s.get('tracks') or [];self.status.set(f"{s.get('phase','review')} | {len(self.tracks)} tracks | {len(s.get('folders',{}))} artist folders");self.render()
  except Exception as e:self.status.set(str(e))
 def render(self):
  self.table.delete(*self.table.get_children());q=self.query.get().lower()
  for t in self.tracks:
   if q and q not in str(t).lower():continue
   act=t.get('action','');act+=(' (duplicate)' if t.get('duplicateOf') else '')
   self.table.insert('', 'end',iid=str(t['id']),values=(t.get('artist'),t.get('title'),t.get('format'),act,t.get('original')))
 def selected(self):
  ids=self.table.selection();return next((t for t in self.tracks if str(t['id'])==ids[0]),None) if ids else None
 def update(self,changes=None,aliases=None):self.job(['--edit'],done=self.refresh,input=json.dumps({'changes':changes or [],'aliases':aliases or {}}))
 def edit_artist(self):
  t=self.selected();
  if not t:return
  s=simpledialog.askstring('Artist',f"First/main artist for {t['title']}:",initialvalue=t['artist']);
  if s:self.update([{'id':t['id'],'artist':s}])
 def rename_group(self):
  t=self.selected();
  if not t:return
  s=simpledialog.askstring('Rename group',f"Rename all '{t['artist']}' songs to:",initialvalue=t['artist']);
  if s:self.update(aliases={t['artist']:s})
 def toggle(self):
  t=self.selected();
  if t:self.update([{'id':t['id'],'action':'skip' if t['action']=='keep' else 'keep'}])
 def action(self,act):
  if act in ('stage','publish','finalize','restore','newbatch') and not messagebox.askyesno('Confirm',f"Run {act}? Review your music and duplicates first."):return
  self.job(['--action',act],done=self.refresh)
 def download(self):
  urls=self.urls.get('1.0','end').strip().splitlines();urls=[u.strip() for u in urls if u.strip()]
  if urls:self.job(['--download',*urls],done=self.refresh)
if __name__=='__main__':UI().mainloop()
