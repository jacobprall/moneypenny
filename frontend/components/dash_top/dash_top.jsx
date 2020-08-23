import React from 'react'
import DashHeader from './dash_header';
import DashNav from './dash_nav'

export default function dash_top() {
  return (
    <header className="main-header">
      <DashHeader />
      <DashNav />
    </header>
  )
}
