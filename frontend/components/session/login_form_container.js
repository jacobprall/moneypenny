import { connect } from 'react-redux';
import SessionForm from './session_form';
// import SessionFormClass from './session_form_class'
import { login, CLEAR_SESSION_ERRORS } from '../../actions/session_actions';



const mapStateToProps = ({ errors }) => ({
  errors: errors.session,
  formType: 'login'
})

const mapDispatchToProps = (dispatch) => ({
  processForm: (user) => (dispatch(login(user))),
  processDemoForm: user => dispatch(login(user)),
  clearErrors: () => dispatch({type: CLEAR_SESSION_ERRORS})
})

export default connect(mapStateToProps, mapDispatchToProps)(SessionForm)