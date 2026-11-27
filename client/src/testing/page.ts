// The page in jsdom: opened on the stand-in for the server (server.ts) at a path, found as a
// person finds what is on it, and worked with events fired at it. The page tests sit in
// src/page/ and end each test with `cleanup` (done here, after each test).
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { afterEach, expect, vi } from 'vitest'

import App from '../App.svelte'
import type { Server } from './server'
import { observers } from './setup'

afterEach(() => cleanup())

/** Opens the page at `path` on `server`, as a browser does that has read nothing of it yet. */
export function open(server: Server, path = '/') {
  history.replaceState(null, '', path)
  vi.stubGlobal('fetch', server.fetch)
  Object.defineProperty(navigator, 'sendBeacon', { configurable: true, value: server.beacon })
  render(App)
}

/**
 * Reads the page anew where it is, as the browser's reload does: what the browser keeps for it
 * (the history entry and its state, localStorage) stays; the page itself is made again.
 */
export function reload(server: Server) {
  // a page that goes away tells it, as a browser does on a reload
  window.dispatchEvent(new Event('pagehide'))
  cleanup()
  vi.stubGlobal('fetch', server.fetch)
  render(App)
}

/** The first element `selector` finds, or null. */
export const $ = (selector: string, within: ParentNode = document) =>
  within.querySelector<HTMLElement>(selector)

/** Every element `selector` finds. */
export const $$ = (selector: string, within: ParentNode = document) => [
  ...within.querySelectorAll<HTMLElement>(selector),
]

/** The element `selector` finds, once there is one. */
export async function found(selector: string, within: ParentNode = document) {
  return waitFor(() => {
    const element = $(selector, within)
    expect(element, selector).not.toBeNull()
    return element as HTMLElement
  })
}

/** Elements whose own text is `text`, whole (as getByText with exact: true). */
export const texts = (text: string | RegExp) => screen.queryAllByText(text)

/** The element whose text is `text`, once there is one. */
export const shown = (text: string | RegExp) => screen.findByText(text)

/** Waits until `check` passes. */
export const until = (check: () => unknown) => waitFor(check)

/** The button named `name`, of those a reader of the screen is told (within `within`). */
export function button(name: string | RegExp, within?: HTMLElement) {
  const buttons = within
    ? screen.getAllByRole('button', { name }).filter((one) => within.contains(one))
    : screen.getAllByRole('button', { name })
  expect(buttons, `the button ${name}`).toHaveLength(1)
  return buttons[0]
}

/** A left click, as a person makes it. */
export async function click(element: Element) {
  await fireEvent.pointerDown(element, { button: 0, pointerType: 'mouse' })
  await fireEvent.mouseDown(element, { button: 0 })
  await fireEvent.pointerUp(element, { button: 0, pointerType: 'mouse' })
  await fireEvent.mouseUp(element, { button: 0 })
  await fireEvent.click(element, { button: 0 })
}

/** A right click: the menu of the context. */
export async function rightClick(element: Element) {
  await fireEvent.pointerDown(element, { button: 2, pointerType: 'mouse' })
  await fireEvent.mouseDown(element, { button: 2 })
  await fireEvent.pointerUp(element, { button: 2, pointerType: 'mouse' })
  await fireEvent.mouseUp(element, { button: 2 })
  await fireEvent.contextMenu(element, { button: 2 })
}

/** Types `value` into a field, as a person does: the field takes it, and says so. */
export async function fill(field: Element, value: string) {
  await fireEvent.input(field, { target: { value } })
}

/** A key pressed and let go at `element` (the document, when none). */
export async function press(
  key: string,
  element: Element = document.activeElement ?? document.body,
  options = {},
) {
  await fireEvent.keyDown(element, { key, ...options })
  await fireEvent.keyUp(element, { key, ...options })
}

/**
 * Tells the page that `elements` came into sight, as the browser's IntersectionObserver does
 * when they are scrolled to: each observer that watches one hears of it.
 */
export function seen(...elements: Element[]) {
  for (const observer of observers) {
    const entries = elements
      .filter((element) => observer.watched.has(element))
      .map(
        (target) =>
          ({ target, isIntersecting: true, intersectionRatio: 1 }) as IntersectionObserverEntry,
      )
    if (entries.length) observer.callback(entries, observer as unknown as IntersectionObserver)
  }
}

/** A touch event of `type` at `element`, with the fingers `touches` and those that moved. */
export function touch(
  element: Element,
  type: string,
  touches: { clientX: number; clientY: number }[],
  changed = touches,
) {
  const event = new Event(type, { bubbles: true, cancelable: true })
  const points = (list: typeof touches) =>
    list.map((point) => ({ identifier: 1, target: element, ...point }))
  Object.defineProperties(event, {
    touches: { value: points(touches) },
    targetTouches: { value: points(touches) },
    changedTouches: { value: points(changed) },
  })
  element.dispatchEvent(event)
}

/** A finger drawn `dx`, `dy` across `element` from (x, y), in a few moves. */
export function swipe(element: Element, dx: number, dy = 0, x = 20, y = 300) {
  touch(element, 'touchstart', [{ clientX: x, clientY: y }])
  touch(element, 'touchmove', [{ clientX: x + dx * 0.3, clientY: y + dy * 0.3 }])
  touch(element, 'touchmove', [{ clientX: x + dx * 0.7, clientY: y + dy * 0.7 }])
  touch(element, 'touchend', [], [{ clientX: x + dx, clientY: y + dy }])
}

/** Waits a frame and a moment, for what the page does after it has drawn. */
export const settle = () =>
  new Promise<void>((done) => requestAnimationFrame(() => setTimeout(done, 20)))

/** Goes back a page in the history, and waits until the browser has gone there. */
export async function back() {
  await new Promise<void>((done) => {
    window.addEventListener('popstate', () => done(), { once: true })
    history.back()
  })
  await settle()
}

/** Goes forward a page in the history, and waits until the browser has gone there. */
export async function forward() {
  await new Promise<void>((done) => {
    window.addEventListener('popstate', () => done(), { once: true })
    history.forward()
  })
  await settle()
}

/** The path the page is at. */
export const path = () => location.pathname
