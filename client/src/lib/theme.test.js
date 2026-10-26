import { describe, it, expect } from 'vitest'
import { opaqueOver } from './theme.js'

describe('opaqueOver', () => {
  it('keeps an opaque colour as it is', () => {
    expect(opaqueOver('rgb(250, 250, 250)', 'rgb(25, 25, 25)')).toBe('rgb(250, 250, 250)')
  })

  it('blends a translucent overlay onto the colour beneath it', () => {
    // The image viewer backdrop (rgba(0, 0, 0, 0.92)) over the Washi surface.
    expect(opaqueOver('rgba(0, 0, 0, 0.92)', 'rgb(250, 250, 250)')).toBe('rgb(20, 20, 20)')
  })

  it('shows the colour beneath through a fully transparent layer', () => {
    expect(opaqueOver('rgba(0, 0, 0, 0)', 'rgb(25, 25, 25)')).toBe('rgb(25, 25, 25)')
  })
})
