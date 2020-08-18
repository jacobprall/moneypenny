import React from 'react'
import accountsReducer from '../../reducers/accounts_reducer'

export default function account_line_item({account}) {
  return (
    <li key={account.id} className="account-line-item">
     
      <ul className="account-items">
        <li className="account-item">{account.label}</li>
        <li className="account-item">{account.institution}</li>
      </ul>
      <span className="item-balance">{account.balance}</span>
    </li>
  )
}
