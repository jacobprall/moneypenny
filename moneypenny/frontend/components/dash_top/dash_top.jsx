import React from 'react'
import DashHeaderContainer from './dash_header_container.js';
import DashNav from './dash_nav'

export default function dash_top() {
  return (
    <header className="main-header">
      <DashHeaderContainer />
      <DashNav />
    </header>
  )
}
