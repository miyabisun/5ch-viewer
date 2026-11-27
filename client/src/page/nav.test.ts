import { screen } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import { $, click, open, until } from '../testing/page'
import { server } from '../testing/server'

test('nav tabs switch between favorites and register pages', async () => {
  const api = server().on('/api/favorites', [])
  open(api, '/')

  // Default: favorites page. The register form (mode select) is not shown.
  const favorites = screen.getByTestId('tab-favorites')
  const register = screen.getByTestId('tab-register')
  expect(favorites.classList).toContain('active')
  expect($('.add')).toBeNull()

  // Switch to register page: the search/url form appears.
  await click(register)
  expect(register.classList).toContain('active')
  await until(() => expect($('.add')).not.toBeNull())
  expect(screen.getByPlaceholderText('キーワードで検索')).toBeTruthy()

  // Switch back to favorites: the form disappears.
  await click(favorites)
  expect(favorites.classList).toContain('active')
  expect($('.add')).toBeNull()
})
