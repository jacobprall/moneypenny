import React from 'react'
import { NavLink } from 'react-router-dom'


export default function dash_nav() {
  return (
    <div className="dash-nav">
      <ul className="dash-nav-links">
        <li className="dash-nav-link"><NavLink activeClassName= "selected" exact to="/overview" >Overview</NavLink></li>
        {/* <li className="dash-nav-link"><NavLink activeClassName="selected" to="/overview/trends" >Trends</NavLink></li> */}
        <li className="dash-nav-link"><NavLink activeClassName= "selected" to="/overview/transactions" >Transactions</NavLink></li>
        <li className="dash-nav-link"><NavLink activeClassName= "selected" to="/overview/goals" >Goals</NavLink></li>
        <li className="dash-nav-link"><NavLink activeClassName= "selected" to="/overview/bills" >Bills</NavLink></li>
      </ul>
    </div>
  )
}
