import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "../../lib/utils"
import tokens from "../../../../shared/components.json"

// Source-owned shadcn-style control. Rune uses an independent native renderer
// with the same 4px, looping-dash and inset-active interaction contract.
const buttonVariants = cva(
  "gf-forge-button relative isolate inline-flex select-none items-center justify-center border text-xs font-semibold tracking-[0.07em] uppercase transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-[3px] focus-visible:outline-[#f7d6a9] disabled:pointer-events-none disabled:opacity-45",
  {variants:{
    variant:{
      primary:"border-[#e2383a] bg-gradient-to-b from-[#af1524] to-[#4a0815] text-[#fff6ec] [--forge-edge:#ef4545] [--forge-tip:#fff0d7]",
      secondary:"border-[#ad825b] bg-gradient-to-b from-[#221317] to-[#0d070b] text-[#f6ead9] [--forge-edge:#daac75] [--forge-tip:#fff4db]",
      ghost:"border-[#76363d] bg-[#120a0d] text-[#e9c5b6] [--forge-edge:#ff657a] [--forge-tip:#fff0d7]"
    },
    size:{default:"",sm:"text-[11px]",lg:"text-sm"}
  }, defaultVariants:{variant:"secondary",size:"default"}}
)

type Ripple = {id:number,x:number,y:number,diameter:number}
type ButtonProps = React.ComponentProps<"button"> & VariantProps<typeof buttonVariants> & {
  active?:boolean
  forgeSize?: keyof typeof tokens.buttons
  icon?:React.ReactNode
}

/**
 * Active is a persistent toggle state, not the same as primary styling.
 * One 20/80 SVG dash traces the actual rounded perimeter every 2.1s on hover;
 * inner-glow is stable while active; a separate 620ms ripple follows clicks.
 */
export function Button({className,variant,size,forgeSize="regular",icon,active=false,onPointerDown,onClick,children,disabled,style,...props}:ButtonProps){
  const geometry=tokens.buttons[forgeSize]
  const radius=Math.min(geometry.radius,geometry.height/2)
  const controlStyle:React.CSSProperties={height:geometry.height,minWidth:geometry.minimum_width,
    borderRadius:radius,paddingInline:geometry.padding_x,
    fontSize:geometry.font_px,"--forge-radius":`${radius}px`,
    "--forge-duration":`${tokens.interaction.trace_revolution_seconds}s`,
    "--forge-ripple-duration":`${tokens.interaction.ripple_seconds}s`,...style} as React.CSSProperties
  const [ripples,setRipples]=React.useState<Ripple[]>([])
  const id=React.useId().replaceAll(":", "")
  const nextId=React.useRef(0)
  const timers=React.useRef<number[]>([])
  React.useEffect(()=>()=>timers.current.forEach(t=>window.clearTimeout(t)),[])
  function rippleAt(x:number,y:number,width:number,height:number){
    if(window.matchMedia("(prefers-reduced-motion: reduce)").matches) return
    const diameter=Math.hypot(width,height)*2
    const item={id:++nextId.current,x,y,diameter}
    setRipples(old=>[...old,item].slice(-5))
    timers.current.push(window.setTimeout(()=>setRipples(old=>old.filter(r=>r.id!==item.id)),tokens.interaction.ripple_seconds*1000))
  }
  function down(event:React.PointerEvent<HTMLButtonElement>){
    onPointerDown?.(event)
    if(disabled||event.defaultPrevented||event.button!==0) return
    const r=event.currentTarget.getBoundingClientRect()
    rippleAt(event.clientX-r.left,event.clientY-r.top,r.width,r.height)
  }
  function click(event:React.MouseEvent<HTMLButtonElement>){
    // detail=0 is keyboard activation: center the ripple without synthesizing a pointer.
    if(!disabled&&event.detail===0){
      const r=event.currentTarget.getBoundingClientRect()
      rippleAt(r.width/2,r.height/2,r.width,r.height)
    }
    onClick?.(event)
  }
  return <button {...props} type={props.type ?? "button"} disabled={disabled}
    aria-pressed={active ? true : props["aria-pressed"]}
    data-active={active ? "true" : undefined}
    className={cn(buttonVariants({variant,size}),className)} style={controlStyle}
    onPointerDown={down} onClick={click}>
    <svg className="gf-forge-button-trace" aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id={`${id}-trace`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="var(--forge-edge)"/>
          <stop offset="100%" stopColor="var(--forge-tip)"/>
        </linearGradient>
      </defs>
      <rect className="gf-trace-glow" x="1.5" y="1.5" rx={Math.max(0,radius-1.5)} pathLength="100"/>
      <rect className="gf-trace-core" x="1.5" y="1.5" rx={Math.max(0,radius-1.5)} pathLength="100" stroke={`url(#${id}-trace)`}/>
    </svg>
    <span className="gf-forge-ripple-clip" aria-hidden="true">
      {ripples.map(r=><span key={r.id} className="gf-forge-ripple" style={{
        width:r.diameter,height:r.diameter,left:r.x-r.diameter/2,top:r.y-r.diameter/2
      }}/>) }
    </span>
    <span className="gf-forge-button-caption">{icon}{children}</span>
  </button>
}
export {buttonVariants}
