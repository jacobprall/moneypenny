import React from 'react'
import {Link} from 'react-router-dom'
import { useSelector, useDispatch } from 'react-redux'
import AccountFormContainer from '../accounts/account_form_modals/account_form_container'
import { logout } from '../../actions/session_actions'
import { openModal } from '../../actions/modal_actions'

export default function DashHeader() {

  const selectedData = useSelector((state) => ({
    passedAccount: {
      'account_category': 'Cash',
      'balance': 0,
      'debit': true,
      'institution': "None",
      'label': "",
      'user_id': state.session.id
    }
  }));

  const { passedAccount } = selectedData
  const dispatch = useDispatch()
  const logOut = () => dispatch(logout())
  const modalOpener = (formType, component, payload) => dispatch(openModal(formType, component, payload))

  const eventHandler = (e) => {
    e.preventDefault();
    logOut();
  }



  return (
    <div className="top-header">
      <div className="main-logo">
        <img className='leaf' src={window.logo} alt="leaf" />
        <Link to="/overview">moneypenny</Link>
      </div>
      <ul className="header-links">
        <li>
          <Link to="#" onClick={() => modalOpener('new', AccountFormContainer, passedAccount )}>+ADD ACCOUNT</Link>
        </li>
        <li>
          <a href="https://github.com/jacobprall/moneypenny" target="_blank">GITHUB</a>
        </li>
        <li>
          <a href="https://www.linkedin.com/in/jacob-prall-01abb867/" target="_blank">LINKEDIN</a>
        </li>
        <li>
          <a href="/" onClick={eventHandler}>LOGOUT</a>
        </li>
      </ul>
    </div>
  )

}
