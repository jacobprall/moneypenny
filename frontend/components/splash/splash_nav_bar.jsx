import React from 'react'
import {Link} from 'react-router-dom'
import Scroll from 'react-scroll'
export default function nav_bar() {
  const ScrollLink = Scroll.Link;
  return (

      <div className="app-header">
        <header className='splash-header'>
          <div className="color-bar"></div>
          <nav className = "splash-nav">
            <div className="logo">
              <img className='leaf' src={window.logo} alt="leaf"/>
              <h2 className="name">moneypenny</h2>
            </div>
            <ul className="splash-links">
              <li> 
                <ScrollLink 
                  to="scroll-to"
                  spy={true}
                  smooth={true}
                  duration={500}
                  className="splash-link"
                >
                  How it works
                </ScrollLink>
              </li>

              <li> <a className="splash-link" target="_blank" href="https://www.linkedin.com/in/jacob-prall-01abb867/"> LinkedIn</a></li>
              <li> <a href="https://github.com/jacobprall/moneypenny" target="_blank" className="splash-link">Github</a> </li>
            </ul>
            <div className="splash-btns">
              <button className="sign-up"><Link to="/signup">Sign up</Link></button>
              <button className="login"><Link to="/login">Sign in</Link></button>
            </div>
          </nav>
        </header>
      </div>
  )
}
