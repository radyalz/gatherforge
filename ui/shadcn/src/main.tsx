import * as React from "react"
import { createRoot } from "react-dom/client"
import { Button } from "./components/ui/button"
import { Download, ScanSearch, Settings2 } from "lucide-react"
import "./styles.css"

function App(){
  const [playing,setPlaying]=React.useState(false)
  const [events,setEvents]=React.useState(0)
  return <main className="min-h-screen bg-[#090608] p-12 text-[#f9eee5]">
    <header className="mb-8 max-w-[980px] border-b border-[#6e2630] pb-5">
      <p className="mb-1 text-xs uppercase tracking-[.3em] text-[#d5a66d]">GatherForge / Interaction System</p>
      <h1 className="text-3xl font-bold tracking-tight">Border Trace &amp; Inner Glow</h1>
      <p className="mt-2 text-sm text-[#c2a9a5]">A repeating SVG border stroke, a pressed-state inset glow, and pointer ripple. No extra decoration before labels.</p>
    </header>
    <section className="max-w-[980px] border border-[#7c4a40] bg-[#10090d] p-7">
      <p className="mb-6 text-sm text-[#c8b2a6]">Hover to start the infinitely looping trace. Click to see a ripple. Toggle Play to keep the inner glow active.</p>
      <div className="flex flex-wrap gap-5 items-center">
        <Button variant="primary" onClick={()=>setEvents(e=>e+1)}><ScanSearch size={15}/> Scan Library</Button>
        <Button variant="secondary" onClick={()=>setEvents(e=>e+1)}><Download size={15}/> Download</Button>
        <Button variant="ghost" onClick={()=>setEvents(e=>e+1)}><Settings2 size={15}/> Settings</Button>
        <Button variant="primary" active={playing} onClick={()=>setPlaying(x=>!x)}>{playing?"Now Playing":"Play Stream"}</Button>
        <Button disabled>Disabled</Button>
      </div>
      <p className="mt-7 text-xs text-[#d4a46f]">Actions: {events} · Playing: {playing?"yes":"no"}</p>
    </section>
  </main>
}
createRoot(document.getElementById("root")!).render(<React.StrictMode><App/></React.StrictMode>)
