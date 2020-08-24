import React from 'react'
import AccountFormContainer from '../accounts/account_form_modals/account_form_container'
import { openModal } from '../../actions/modal_actions'
import { deleteAccount } from '../../actions/account_actions'
import { useDispatch } from 'react-redux'

export default function account_line_item({account, commaFormat}) {
  const dispatch = useDispatch();
  const modalOpener = (formType, component, account) => dispatch(openModal(formType, component, account));
  const accountDeleter = (accountId) => dispatch(deleteAccount(accountId));
  
  return (
    <li key={account.id} className="account-line-item">
     
      <ul className="account-items" >
        <li className="account-item">{account.label}</li>
        <li className="account-institution">{account.institution}</li>
      </ul>
      <div className="line-item-right">
        <span className="item-balance">{`$${commaFormat((account.balance.toFixed(2).toString()))}`}</span>

          <img src={window.pencil} alt="pencil" className="pencil" onClick={() => modalOpener('edit', AccountFormContainer, account)}/>
          <img src={gwindow.trashCan} alt="x" className="x" onClick={() => accountDeleter(account.id)}/>

      </div>
      

    </li>
  )
}
