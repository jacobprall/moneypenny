import React from 'react'
import {Link} from 'react-router-dom'
export default function nav_bar() {

  return (
    // <div className ="background">
      <div className="app-header">
        <header className='splash-header'>
          <div className="color-bar"></div>
          <nav className = "splash-nav">
            <div className="logo">
              <img className='leaf' src={window.logo} alt="leaf"/>
              <h2 className="name">moneypenny</h2>
            </div>
            <ul className="splash-links">
              <li> <a href="#" className="splash-link" >How it works</a> </li>
              <li> <a className="splash-link" href="#"> LinkedIn</a></li>
              <li> <a href="#" className="splash-link">Github</a> </li>
            </ul>
            <div className="splash-btns">
              <button className="sign-up"><Link to="/signup">Sign up</Link></button>
              <button className="login"><Link to="/login">Sign in</Link></button>
            </div>
          </nav>
        </header>
      </div>
    // </div>
  )
}
