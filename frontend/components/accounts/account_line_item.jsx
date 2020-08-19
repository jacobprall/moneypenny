import React from 'react'

export default function account_line_item({account, openModal}) {
  return (
    <li key={account.id} className="account-line-item">
     
      <ul className="account-items" onClick={() => openModal('edit')}>
        <li className="account-item">{account.label}</li>
        <li className="account-institution">{account.institution}</li>
      </ul>
      <span className="item-balance">{account.balance}</span>
      {/* <span onClick={openModal('edit')}>Edit</span> */}
    </li>
  )
}
