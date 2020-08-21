import React from 'react'

export default function account_line_item({account, openModal, deleteAccount, commaFormat}) {
  return (
    <li key={account.id} className="account-line-item">
     
      <ul className="account-items" >
        <li className="account-item">{account.label}</li>
        <li className="account-institution">{account.institution}</li>
      </ul>
      <div className="line-item-right">
        <span className="item-balance">{`$${commaFormat((account.balance.toFixed(2).toString()))}`}</span>
        {/* <div className="account-edit-options"> */}
          <img src={`${window.pencil}`} alt="pencil" className="pencil" onClick={() => openModal('edit account', account)}/>
          <img src={`${window.trashCan}`} alt="x" className="x" onClick={() => deleteAccount(account.id)}/>
        {/* </div> */}
      </div>
      

    </li>
  )
}
