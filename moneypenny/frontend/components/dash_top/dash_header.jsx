import React from 'react'
import {Link} from 'react-router-dom'
export default function DashHeader({ logout }) {

  const eventHandler = (e) => {
    e.preventDefault();
    logout();
  }

  return (
    <div className="top-header">
      <div className="main-logo">
        <img className='leaf' src={window.logo} alt="leaf" />
        <Link to="/overview">moneypenny</Link>
      </div>
      <ul className="header-links">
        <li>
          <Link to="/overview/aa">+ADD ACCOUNT</Link>
        </li>
        <li>
          <a href="#">GITHUB</a>
        </li>
        <li>
          <a href="#">LINKEDIN</a>
        </li>
        <li>
          <a href="/" onClick={eventHandler}>LOGOUT</a>
        </li>
      </ul>
    </div>
  )

}
